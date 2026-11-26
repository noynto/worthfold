use std::sync::Arc;

use asset::AssetProvider;
use bank::{BankAggregator, BankConnectionProvider};
use category::{CategoryProvider, matching_category};
use chrono::{Days, NaiveDate};
use shared::{Money, TransactionId, ValuationId};
use transaction::{PendingTransaction, Transaction, TransactionProvider, TransactionSource};
use valuation::{Valuation, ValuationProvider};

use crate::UseCaseError;

/// Première lecture d'un compte : les banques ne rendent guère plus de 90 jours d'historique.
const FIRST_SYNC_DAYS: u64 = 90;
/// Chaque lecture recouvre les derniers jours déjà lus : une opération qui passe de « en attente » à
/// « comptabilisée » après coup n'est pas ratée. Les doublons sont écartés par empreinte.
const OVERLAP_DAYS: u64 = 7;

pub struct SyncBankAccounts {
    pub aggregator: Option<Arc<dyn BankAggregator>>,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
    pub asset_provider: Arc<dyn AssetProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Command {
    pub today: NaiveDate,
    /// Relit tout l'historique disponible (90 jours), sans tenir compte de la dernière lecture.
    /// Utile quand une opération apparaît chez la banque avec une date plus ancienne que le recouvrement.
    /// Les doublons restent écartés par empreinte.
    pub full: bool,
}

/// Ce qu'a donné la lecture d'un compte.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountReport {
    pub label: String,
    pub imported: usize,
    pub already_known: usize,
    /// Les opérations en attente que la banque annonce (elles remplacent celles de la lecture précédente).
    pub pending: usize,
    /// La raison, en français, quand la lecture a échoué (le détail technique est journalisé).
    pub failure: Option<String>,
}

#[derive(Debug, Default)]
pub struct Output {
    pub accounts: Vec<AccountReport>,
    /// Les consentements expirés : leurs comptes ne sont pas lus, il faut les renouveler.
    pub expired: Vec<String>,
    /// Les comptes de la banque pas encore rattachés à un compte de worthfold.
    pub unlinked: usize,
}

impl Output {
    pub fn imported(&self) -> usize {
        self.accounts.iter().map(|a| a.imported).sum()
    }

    pub fn succeeded(&self) -> bool {
        self.expired.is_empty() && self.accounts.iter().all(|a| a.failure.is_none())
    }
}

impl SyncBankAccounts {
    /// Lit les opérations de chaque compte rattaché. Un compte en échec n'arrête pas les autres.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let aggregator = self
            .aggregator
            .as_ref()
            .ok_or_else(|| UseCaseError::refusal(crate::BANK_NOT_CONFIGURED))?;
        let today = command.today;
        let connections = self.bank_provider.read_connections().await?;
        let accounts = self.bank_provider.read_accounts().await?;
        let assets = self.asset_provider.read_all().await?;
        let rules = self.category_provider.read_rules().await?;
        let mut output = Output::default();

        for connection in &connections {
            if connection.days_left(today) < 0 {
                output.expired.push(connection.bank_name.clone());
                continue;
            }
            for account in accounts.iter().filter(|a| a.connection_id == connection.id) {
                let Some(asset) = account
                    .asset_id
                    .as_ref()
                    .and_then(|id| assets.iter().find(|a| &a.id == id))
                else {
                    output.unlinked += 1;
                    continue;
                };
                let last = account.last_synced_on.filter(|_| !command.full);
                let from = last.map_or(today - Days::new(FIRST_SYNC_DAYS), |d| {
                    d - Days::new(OVERLAP_DAYS)
                });
                let mut report = AccountReport {
                    label: account.label.clone(),
                    imported: 0,
                    already_known: 0,
                    pending: 0,
                    failure: None,
                };
                let remote = match aggregator.transactions(&account.uid, from, today).await {
                    Ok(remote) => remote,
                    Err(e) => {
                        tracing::error!(error = ?e, account = %account.label, "Lecture des opérations refusée.");
                        report.failure = Some(
                            "la banque n'a pas rendu les opérations (consentement expiré ou banque indisponible)"
                                .to_owned(),
                        );
                        output.accounts.push(report);
                        continue;
                    }
                };
                let incoming: Vec<Transaction> = remote
                    .into_iter()
                    .filter(|t| t.currency == asset.currency && t.booked_on <= today)
                    .map(|t| Transaction {
                        id: TransactionId::generate(),
                        asset_id: asset.id.clone(),
                        booked_on: t.booked_on,
                        amount: Money::new(t.amount, asset.currency),
                        category_id: matching_category(&t.label, &rules),
                        label: t.label,
                        source: TransactionSource::Bank,
                    })
                    .collect();
                let total = incoming.len();
                let existing = self.transaction_provider.read_of_asset(&asset.id).await?;
                let new = service::transactions::new_transactions(&existing, incoming);
                self.transaction_provider.write_many(&new).await?;
                report.imported = new.len();
                report.already_known = total - new.len();

                // Le solde de la banque devient une valorisation, sauf s'il ne change rien.
                if let Ok(Some(balance)) = aggregator.balance(&account.uid).await {
                    let valuations = self.valuation_provider.read_all().await?;
                    let latest = service::latest_valuation(asset, &valuations, today);
                    if latest.is_none_or(|v| v.value.amount != balance) {
                        self.valuation_provider
                            .write(&Valuation {
                                id: ValuationId::generate(),
                                asset_id: asset.id.clone(),
                                value: Money::new(balance, asset.currency),
                                valued_on: today,
                            })
                            .await?;
                    }
                }
                self.bank_provider.set_synced(&account.uid, today).await?;

                // Les opérations en attente remplacent celles de la lecture précédente. Une liste qu'on ne sait
                // pas rafraîchir est vidée : une opération déjà comptabilisée, affichée encore comme « en attente »,
                // fausserait le solde disponible. Elle ne fait pas échouer la lecture des opérations comptabilisées.
                let known: std::collections::HashSet<String> = existing
                    .iter()
                    .map(Transaction::fingerprint)
                    .chain(new.iter().map(Transaction::fingerprint))
                    .collect();
                let pending: Vec<PendingTransaction> = match aggregator
                    .pending_transactions(&account.uid, today)
                    .await
                {
                    Ok(remote) => remote
                        .into_iter()
                        .filter(|t| t.currency == asset.currency)
                        .map(|t| PendingTransaction {
                            asset_id: asset.id.clone(),
                            booked_on: t.booked_on,
                            amount: Money::new(t.amount, asset.currency),
                            label: t.label,
                        })
                        // La banque rend parfois l'opération en attente et comptabilisée à la fois : une seule fois.
                        .filter(|p| {
                            !known.contains(
                                &Transaction {
                                    id: TransactionId::generate(),
                                    asset_id: p.asset_id.clone(),
                                    booked_on: p.booked_on,
                                    amount: p.amount,
                                    label: p.label.clone(),
                                    source: TransactionSource::Bank,
                                    category_id: None,
                                }
                                .fingerprint(),
                            )
                        })
                        .collect(),
                    Err(e) => {
                        tracing::warn!(error = ?e, account = %account.label, "Lecture des opérations en attente impossible.");
                        Vec::new()
                    }
                };
                report.pending = pending.len();
                self.transaction_provider
                    .replace_pending(&asset.id, &pending)
                    .await?;
                output.accounts.push(report);
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use bank::{BankAccount, BankConnection, RemoteTransaction};
    use rust_decimal::Decimal;
    use shared::{BankConnectionId, Currency};
    use testing::{
        FakeAggregator, InMemoryAssets, InMemoryBank, InMemoryCategories, InMemoryTransactions,
        InMemoryValuations,
    };

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    struct Setup {
        use_case: SyncBankAccounts,
        aggregator: Arc<FakeAggregator>,
        bank: Arc<InMemoryBank>,
        transactions: Arc<InMemoryTransactions>,
        valuations: Arc<InMemoryValuations>,
    }

    fn setup(valid_until: NaiveDate, linked: bool) -> Setup {
        let connection = BankConnection {
            id: BankConnectionId::generate(),
            bank_name: "Banque".into(),
            country: "FR".into(),
            session_id: "s".into(),
            valid_until,
            created_on: day(10, 1),
        };
        let account = BankAccount {
            uid: "u1".into(),
            connection_id: connection.id.clone(),
            identity: "i".into(),
            label: "Courant".into(),
            currency: Currency::Eur,
            asset_id: linked.then(|| "compte".into()),
            last_synced_on: None,
        };
        let aggregator = Arc::new(FakeAggregator::default());
        let bank = Arc::new(InMemoryBank::with(vec![connection], vec![account]));
        let transactions = Arc::new(InMemoryTransactions::default());
        let valuations = Arc::new(InMemoryValuations::default());
        Setup {
            use_case: SyncBankAccounts {
                aggregator: Some(aggregator.clone()),
                bank_provider: bank.clone(),
                asset_provider: Arc::new(InMemoryAssets::with([Asset {
                    id: "compte".into(),
                    name: "Compte".into(),
                    kind: AssetKind::BankAccount,
                    currency: Currency::Eur,
                    bank_name: None,
                    account_number: None,
                }])),
                transaction_provider: transactions.clone(),
                valuation_provider: valuations.clone(),
                category_provider: Arc::new(InMemoryCategories::default()),
            },
            aggregator,
            bank,
            transactions,
            valuations,
        }
    }

    fn remote(d: u32, amount: i64, label: &str) -> RemoteTransaction {
        RemoteTransaction {
            booked_on: day(11, d),
            amount: Decimal::from(amount),
            currency: Currency::Eur,
            label: label.into(),
        }
    }

    #[tokio::test]
    async fn les_operations_en_attente_remplacent_celles_de_la_lecture_precedente() {
        let s = setup(day(12, 31), true);
        *s.aggregator.transactions.lock().unwrap() = vec![remote(5, -700, "Loyer")];
        *s.aggregator.pending.lock().unwrap() = vec![
            remote(24, -60, "CB Essence"),
            remote(25, -5, "CB Boulangerie"),
            // Déjà comptabilisée dans le même temps : comptée une seule fois.
            remote(5, -700, "Loyer"),
        ];
        let first = s
            .use_case
            .handle(Command {
                today: day(11, 25),
                full: false,
            })
            .await
            .unwrap();
        assert_eq!(
            (first.accounts[0].imported, first.accounts[0].pending),
            (1, 2)
        );
        assert_eq!(s.transactions.read_pending().await.unwrap().len(), 2);

        // Lecture suivante : une seule opération en attente reste, la précédente est remplacée.
        *s.aggregator.pending.lock().unwrap() = vec![remote(25, -5, "CB Boulangerie")];
        s.use_case
            .handle(Command {
                today: day(11, 26),
                full: false,
            })
            .await
            .unwrap();
        let pending = s.transactions.read_pending().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].label, "CB Boulangerie");

        // Une panne de la lecture en attente vide la liste (mieux vaut rien qu'un solde faux),
        // sans faire échouer la lecture des opérations comptabilisées.
        *s.aggregator.fail_pending.lock().unwrap() = true;
        let third = s
            .use_case
            .handle(Command {
                today: day(11, 27),
                full: false,
            })
            .await
            .unwrap();
        assert!(third.succeeded());
        assert!(s.transactions.read_pending().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn une_lecture_complete_remonte_a_90_jours_sans_doubler() {
        let s = setup(day(12, 31), true);
        *s.aggregator.transactions.lock().unwrap() = vec![remote(5, -700, "Loyer")];
        s.use_case
            .handle(Command {
                today: day(11, 25),
                full: false,
            })
            .await
            .unwrap();

        let full = s
            .use_case
            .handle(Command {
                today: day(11, 27),
                full: true,
            })
            .await
            .unwrap();
        assert_eq!((full.imported(), full.accounts[0].already_known), (0, 1));
        assert_eq!(
            s.aggregator.requests.lock().unwrap()[1].1,
            day(11, 27) - Days::new(90)
        );
    }

    #[tokio::test]
    async fn importe_les_operations_le_solde_et_ne_les_double_pas() {
        let s = setup(day(12, 31), true);
        *s.aggregator.transactions.lock().unwrap() =
            vec![remote(5, -700, "Loyer"), remote(6, 2000, "Salaire")];
        *s.aggregator.balance.lock().unwrap() = Some(Decimal::from(1300));

        let first = s
            .use_case
            .handle(Command {
                today: day(11, 25),
                full: false,
            })
            .await
            .unwrap();
        assert_eq!((first.imported(), first.succeeded()), (2, true));
        // Première lecture : 90 jours en arrière.
        assert_eq!(
            s.aggregator.requests.lock().unwrap()[0].1,
            day(11, 25) - Days::new(90)
        );
        assert_eq!(s.valuations.read_all().await.unwrap().len(), 1);

        // Deuxième lecture : la banque rend les mêmes opérations, rien n'est doublé, le solde inchangé non plus.
        let second = s
            .use_case
            .handle(Command {
                today: day(11, 26),
                full: false,
            })
            .await
            .unwrap();
        assert_eq!(
            (second.imported(), second.accounts[0].already_known),
            (0, 2)
        );
        assert_eq!(s.transactions.read_all().await.unwrap().len(), 2);
        assert_eq!(s.valuations.read_all().await.unwrap().len(), 1);
        // Et la lecture recouvre les 7 derniers jours déjà lus.
        assert_eq!(
            s.aggregator.requests.lock().unwrap()[1].1,
            day(11, 25) - Days::new(7)
        );
        assert_eq!(
            s.bank.read_accounts().await.unwrap()[0].last_synced_on,
            Some(day(11, 26))
        );
    }

    #[tokio::test]
    async fn ignore_un_consentement_expire_et_un_compte_non_rattache() {
        let expired = setup(day(11, 24), true);
        let output = expired
            .use_case
            .handle(Command {
                today: day(11, 25),
                full: false,
            })
            .await
            .unwrap();
        assert_eq!(output.expired, ["Banque"]);
        assert!(!output.succeeded());
        assert!(expired.aggregator.requests.lock().unwrap().is_empty());

        let unlinked = setup(day(12, 31), false);
        let output = unlinked
            .use_case
            .handle(Command {
                today: day(11, 25),
                full: false,
            })
            .await
            .unwrap();
        assert_eq!((output.unlinked, output.accounts.len()), (1, 0));
    }

    #[tokio::test]
    async fn une_panne_est_comptee_sans_rien_ecrire() {
        let s = setup(day(12, 31), true);
        *s.aggregator.fail.lock().unwrap() = true;
        let output = s
            .use_case
            .handle(Command {
                today: day(11, 25),
                full: false,
            })
            .await
            .unwrap();
        assert!(!output.succeeded());
        assert!(output.accounts[0].failure.is_some());
        assert_eq!(
            s.bank.read_accounts().await.unwrap()[0].last_synced_on,
            None
        );
    }
}
