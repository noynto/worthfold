use std::sync::Arc;

use asset::{AssetKind, AssetProvider};
use category::{CategoryProvider, matching_category};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{AssetId, Money, TransactionId, ValuationId};
use transaction::{Transaction, TransactionProvider, TransactionSource};
use valuation::{Valuation, ValuationProvider};

use crate::{NOT_FOUND, UseCaseError};

pub struct ImportTransactions {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Command {
    pub asset_id: AssetId,
    /// Le contenu du fichier CSV exporté de la banque.
    pub statement: String,
    /// Le solde du compte juste après la dernière opération du relevé, si on le connaît.
    /// Il devient une valorisation du compte, donc il entre dans le patrimoine net.
    pub closing_balance: Option<Decimal>,
    /// La date du jour, fournie par l'appelant : un relevé ne peut pas contenir d'opération future.
    pub today: NaiveDate,
}

pub struct Output {
    /// Les opérations nouvelles, enregistrées.
    pub imported: usize,
    /// Les opérations que le compte connaissait déjà (relevé qui en recouvre un autre).
    pub already_known: usize,
    /// La valorisation créée à partir du solde indiqué, s'il y en avait un.
    pub balance: Option<Valuation>,
}

impl ImportTransactions {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        // 1. Résolution du compte : seul un compte bancaire a des opérations.
        let asset = self
            .asset_provider
            .read(&command.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        if asset.kind != AssetKind::BankAccount {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'est pas un compte bancaire : l'import de relevé ne concerne que les comptes bancaires.",
                asset.name
            )));
        }
        // 2. Lecture du fichier (règle pure, message prêt à afficher).
        let lines = service::bank_csv::parse(&command.statement).map_err(UseCaseError::refusal)?;
        if let Some(future) = lines
            .iter()
            .map(|l| l.date)
            .filter(|d| *d > command.today)
            .max()
        {
            return Err(UseCaseError::refusal(format!(
                "Le relevé contient une opération datée du {}, dans le futur : vérifie le fichier.",
                future.format("%d/%m/%Y")
            )));
        }
        let last_date = lines.iter().map(|l| l.date).max();
        // 3. Ne garder que ce que le compte ne connaît pas encore.
        // Les opérations arrivent déjà classées quand une règle les reconnaît.
        let rules = self.category_provider.read_rules().await?;
        let incoming: Vec<Transaction> = lines
            .into_iter()
            .map(|line| Transaction {
                id: TransactionId::generate(),
                asset_id: asset.id.clone(),
                booked_on: line.date,
                amount: Money::new(line.amount, asset.currency),
                category_id: matching_category(&line.label, &rules),
                label: line.label,
                source: TransactionSource::Csv,
            })
            .collect();
        let total = incoming.len();
        let existing = self.transaction_provider.read_of_asset(&asset.id).await?;
        let new = service::transactions::new_transactions(&existing, incoming);
        // 4. Écriture : les transactions (toutes ou aucune), puis le solde.
        self.transaction_provider.write_many(&new).await?;
        let balance = match (command.closing_balance, last_date) {
            (Some(amount), Some(valued_on)) => {
                let valuation = Valuation {
                    id: ValuationId::generate(),
                    asset_id: asset.id,
                    value: Money::new(amount, asset.currency),
                    valued_on,
                };
                self.valuation_provider.write(&valuation).await?;
                Some(valuation)
            }
            _ => None,
        };
        Ok(Output {
            imported: new.len(),
            already_known: total - new.len(),
            balance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::Asset;
    use category::CategoryRule;
    use shared::CategoryRuleId;
    use shared::Currency;
    use testing::{InMemoryAssets, InMemoryCategories, InMemoryTransactions, InMemoryValuations};

    const STATEMENT: &str = "Date;Libellé;Montant\n\
                             05/11/2026;Loyer;-700,00\n\
                             06/11/2026;Salaire;2 000,00\n";

    struct Setup {
        use_case: ImportTransactions,
        transactions: Arc<InMemoryTransactions>,
        valuations: Arc<InMemoryValuations>,
    }

    fn setup(kind: AssetKind) -> Setup {
        setup_with_rules(kind, [])
    }

    fn setup_with_rules(kind: AssetKind, rules: impl IntoIterator<Item = CategoryRule>) -> Setup {
        let asset = Asset {
            id: "a".into(),
            name: "Compte courant".into(),
            kind,
            currency: Currency::Eur,
        };
        let transactions = Arc::new(InMemoryTransactions::default());
        let valuations = Arc::new(InMemoryValuations::default());
        Setup {
            use_case: ImportTransactions {
                asset_provider: Arc::new(InMemoryAssets::with([asset])),
                transaction_provider: transactions.clone(),
                valuation_provider: valuations.clone(),
                category_provider: Arc::new(InMemoryCategories::with([], rules)),
            },
            transactions,
            valuations,
        }
    }

    fn command(statement: &str, closing_balance: Option<Decimal>) -> Command {
        Command {
            asset_id: "a".into(),
            statement: statement.into(),
            closing_balance,
            today: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
        }
    }

    #[tokio::test]
    async fn importe_les_operations_dans_la_devise_du_compte() {
        let s = setup(AssetKind::BankAccount);
        let output = s.use_case.handle(command(STATEMENT, None)).await.unwrap();

        assert_eq!((output.imported, output.already_known), (2, 0));
        assert!(output.balance.is_none());
        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].label, "Loyer");
        assert_eq!(
            stored[0].amount,
            Money::new(Decimal::from(-700), Currency::Eur)
        );
        assert!(s.valuations.read_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn classe_a_l_import_les_operations_que_les_regles_reconnaissent() {
        let rule = CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: "logement".into(),
            pattern: "loyer".into(),
        };
        let s = setup_with_rules(AssetKind::BankAccount, [rule]);
        s.use_case.handle(command(STATEMENT, None)).await.unwrap();

        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored[0].category_id, Some("logement".into())); // « Loyer »
        assert_eq!(stored[1].category_id, None); // « Salaire »
    }

    #[tokio::test]
    async fn reimporter_le_meme_releve_ne_cree_pas_de_doublon() {
        let s = setup(AssetKind::BankAccount);
        s.use_case.handle(command(STATEMENT, None)).await.unwrap();
        let output = s.use_case.handle(command(STATEMENT, None)).await.unwrap();

        assert_eq!((output.imported, output.already_known), (0, 2));
        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[tokio::test]
    async fn le_solde_indique_devient_une_valorisation_a_la_date_de_la_derniere_operation() {
        let s = setup(AssetKind::BankAccount);
        let output = s
            .use_case
            .handle(command(STATEMENT, Some(Decimal::from(1_250))))
            .await
            .unwrap();

        let balance = output.balance.unwrap();
        assert_eq!(
            balance.value,
            Money::new(Decimal::from(1_250), Currency::Eur)
        );
        assert_eq!(
            balance.valued_on,
            NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()
        );
        assert_eq!(s.valuations.read_all().await.unwrap(), vec![balance]);
    }

    #[tokio::test]
    async fn refuse_un_actif_qui_n_est_pas_un_compte_bancaire() {
        let s = setup(AssetKind::SavingsBook);
        let result = s.use_case.handle(command(STATEMENT, None)).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_un_fichier_illisible_sans_rien_enregistrer() {
        let s = setup(AssetKind::BankAccount);
        let result = s.use_case.handle(command("n'importe quoi", None)).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_une_operation_dans_le_futur() {
        let s = setup(AssetKind::BankAccount);
        let statement = "Date;Libellé;Montant\n11/11/2026;Demain;-1,00\n";
        let result = s.use_case.handle(command(statement, None)).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_un_compte_inconnu() {
        let s = setup(AssetKind::BankAccount);
        let mut unknown = command(STATEMENT, None);
        unknown.asset_id = "inconnu".into();
        assert!(matches!(
            s.use_case.handle(unknown).await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
