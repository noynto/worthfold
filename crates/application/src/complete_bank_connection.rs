use std::sync::Arc;

use bank::{BankAccount, BankAggregator, BankConnection, BankConnectionProvider};
use chrono::NaiveDate;
use shared::BankConnectionId;

use crate::UseCaseError;

pub struct CompleteBankConnection {
    pub aggregator: Option<Arc<dyn BankAggregator>>,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
}

pub struct Command {
    /// Le code que la banque a renvoyé.
    pub code: String,
    /// L'état que la banque a renvoyé : il doit correspondre à une demande en cours.
    pub state: String,
    pub today: NaiveDate,
}

pub struct Output {
    pub connection: BankConnection,
    pub accounts: usize,
    /// Des comptes déjà rattachés ont été reconnus : le consentement est un renouvellement.
    pub renewed: usize,
}

impl CompleteBankConnection {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let aggregator = self
            .aggregator
            .as_ref()
            .ok_or_else(|| UseCaseError::refusal(crate::BANK_NOT_CONFIGURED))?;
        let pending = self
            .bank_provider
            .take_pending(&command.state)
            .await?
            .ok_or_else(|| {
                UseCaseError::refusal(
                    "Cette demande de connexion est inconnue ou déjà utilisée : recommence depuis la page Banque.",
                )
            })?;
        let session = aggregator
            .create_session(&command.code)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Création de la session bancaire refusée.");
                UseCaseError::refusal(
                    "La banque n'a pas confirmé le consentement : recommence depuis la page Banque.",
                )
            })?;

        let connection = BankConnection {
            id: BankConnectionId::generate(),
            bank_name: pending.bank_name,
            country: pending.country,
            session_id: session.session_id,
            valid_until: session.valid_until,
            created_on: command.today,
        };
        // Un renouvellement : les comptes déjà connus (même identité) gardent leur rattachement et leur
        // date de synchronisation ; l'ancien consentement de cette banque est remplacé.
        let known = self.bank_provider.read_accounts().await?;
        let old_connections: Vec<_> = self
            .bank_provider
            .read_connections()
            .await?
            .into_iter()
            .filter(|c| c.bank_name == connection.bank_name && c.country == connection.country)
            .collect();
        let mut renewed = 0;
        let accounts: Vec<BankAccount> = session
            .accounts
            .into_iter()
            .map(|remote| {
                let previous = known.iter().find(|a| {
                    a.identity == remote.identity
                        && old_connections.iter().any(|c| c.id == a.connection_id)
                });
                renewed += usize::from(previous.is_some_and(|p| p.asset_id.is_some()));
                BankAccount {
                    uid: remote.uid,
                    connection_id: connection.id.clone(),
                    identity: remote.identity,
                    label: remote.label,
                    currency: remote.currency,
                    asset_id: previous.and_then(|p| p.asset_id.clone()),
                    last_synced_on: previous.and_then(|p| p.last_synced_on),
                }
            })
            .collect();
        self.bank_provider
            .write_connection(&connection, &accounts)
            .await?;
        for old in old_connections {
            self.bank_provider.delete_connection(&old.id).await?;
        }
        Ok(Output {
            accounts: accounts.len(),
            connection,
            renewed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bank::{PendingAuthorization, RemoteAccount, RemoteSession};
    use shared::Currency;
    use testing::{FakeAggregator, InMemoryBank};

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn session(uid: &str) -> RemoteSession {
        RemoteSession {
            session_id: "s".into(),
            valid_until: day(2, 23),
            accounts: vec![RemoteAccount {
                uid: uid.into(),
                identity: "hash-1".into(),
                label: "Compte courant".into(),
                currency: Currency::Eur,
            }],
        }
    }

    async fn setup(bank: Arc<InMemoryBank>, uid: &str) -> CompleteBankConnection {
        let aggregator = FakeAggregator::default();
        *aggregator.session.lock().unwrap() = Some(session(uid));
        bank.write_pending(&PendingAuthorization {
            state: "ok".into(),
            bank_name: "Banque".into(),
            country: "FR".into(),
            created_on: day(11, 25),
        })
        .await
        .unwrap();
        CompleteBankConnection {
            aggregator: Some(Arc::new(aggregator)),
            bank_provider: bank,
        }
    }

    fn command(state: &str) -> Command {
        Command {
            code: "c".into(),
            state: state.into(),
            today: day(11, 25),
        }
    }

    #[tokio::test]
    async fn enregistre_le_consentement_et_ses_comptes() {
        let bank = Arc::new(InMemoryBank::default());
        let output = setup(bank.clone(), "u1")
            .await
            .handle(command("ok"))
            .await
            .unwrap();
        assert_eq!((output.accounts, output.renewed), (1, 0));
        assert_eq!(bank.read_accounts().await.unwrap()[0].asset_id, None);
    }

    #[tokio::test]
    async fn refuse_un_etat_inconnu_et_un_etat_deja_utilise() {
        let bank = Arc::new(InMemoryBank::default());
        let use_case = setup(bank, "u1").await;
        assert!(matches!(
            use_case.handle(command("forge")).await,
            Err(UseCaseError::Refusal(_))
        ));
        use_case.handle(command("ok")).await.unwrap();
        assert!(matches!(
            use_case.handle(command("ok")).await,
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[tokio::test]
    async fn un_renouvellement_garde_le_rattachement_et_remplace_l_ancien_consentement() {
        let bank = Arc::new(InMemoryBank::default());
        setup(bank.clone(), "u1")
            .await
            .handle(command("ok"))
            .await
            .unwrap();
        bank.link_account("u1", Some(&"compte".into()))
            .await
            .unwrap();
        bank.set_synced("u1", day(11, 20)).await.unwrap();

        let use_case = setup(bank.clone(), "u2").await;
        let output = use_case.handle(command("ok")).await.unwrap();

        assert_eq!(output.renewed, 1);
        let accounts = bank.read_accounts().await.unwrap();
        assert_eq!(accounts.len(), 1, "l'ancien consentement est remplacé");
        assert_eq!(accounts[0].uid, "u2");
        assert_eq!(accounts[0].asset_id, Some("compte".into()));
        assert_eq!(accounts[0].last_synced_on, Some(day(11, 20)));
        assert_eq!(bank.read_connections().await.unwrap().len(), 1);
    }
}
