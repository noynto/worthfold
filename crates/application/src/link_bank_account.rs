use std::sync::Arc;

use asset::{AssetKind, AssetProvider};
use bank::BankConnectionProvider;
use shared::AssetId;

use crate::{NOT_FOUND, UseCaseError};

pub struct LinkBankAccount {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
}

pub struct Command {
    pub uid: String,
    /// Le compte de worthfold qui reçoit les opérations ; `None` détache.
    pub asset_id: Option<AssetId>,
}

impl LinkBankAccount {
    pub async fn handle(&self, command: Command) -> Result<(), UseCaseError> {
        let accounts = self.bank_provider.read_accounts().await?;
        let account = accounts
            .iter()
            .find(|a| a.uid == command.uid)
            .ok_or_else(|| {
                UseCaseError::refusal(
                    "Ce compte de la banque n'existe plus : renouvelle la connexion.",
                )
            })?;
        if let Some(asset_id) = &command.asset_id {
            let asset = self
                .asset_provider
                .read(asset_id)
                .await?
                .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
            if asset.kind != AssetKind::BankAccount {
                return Err(UseCaseError::refusal(format!(
                    "« {} » n'est pas un compte bancaire.",
                    asset.name
                )));
            }
            if asset.currency != account.currency {
                return Err(UseCaseError::refusal(format!(
                    "« {} » n'est pas dans la même devise que le compte de la banque.",
                    asset.name
                )));
            }
            if accounts
                .iter()
                .any(|a| a.uid != account.uid && a.asset_id.as_ref() == Some(asset_id))
            {
                return Err(UseCaseError::refusal(format!(
                    "« {} » reçoit déjà les opérations d'un autre compte de la banque : les mélanger fausserait le solde.",
                    asset.name
                )));
            }
        }
        self.bank_provider
            .link_account(&command.uid, command.asset_id.as_ref())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::Asset;
    use bank::BankAccount;
    use shared::{BankConnectionId, Currency};
    use testing::{InMemoryAssets, InMemoryBank};

    fn account(uid: &str) -> BankAccount {
        BankAccount {
            uid: uid.into(),
            connection_id: BankConnectionId::generate(),
            identity: uid.into(),
            label: uid.into(),
            currency: Currency::Eur,
            asset_id: None,
            last_synced_on: None,
        }
    }

    fn setup() -> (LinkBankAccount, Arc<InMemoryBank>) {
        let asset = |id: &str, kind| Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        let bank = Arc::new(InMemoryBank::with(
            vec![],
            vec![account("u1"), account("u2")],
        ));
        (
            LinkBankAccount {
                asset_provider: Arc::new(InMemoryAssets::with([
                    asset("compte", AssetKind::BankAccount),
                    asset("livret", AssetKind::SavingsBook),
                ])),
                bank_provider: bank.clone(),
            },
            bank,
        )
    }

    fn link(uid: &str, asset: &str) -> Command {
        Command {
            uid: uid.into(),
            asset_id: Some(asset.into()),
        }
    }

    #[tokio::test]
    async fn rattache_puis_detache() {
        let (use_case, bank) = setup();
        use_case.handle(link("u1", "compte")).await.unwrap();
        assert_eq!(
            bank.read_accounts().await.unwrap()[0].asset_id,
            Some("compte".into())
        );
        use_case
            .handle(Command {
                uid: "u1".into(),
                asset_id: None,
            })
            .await
            .unwrap();
        assert_eq!(bank.read_accounts().await.unwrap()[0].asset_id, None);
    }

    #[tokio::test]
    async fn refuse_un_livret_un_compte_deja_pris_et_un_compte_inconnu() {
        let (use_case, _) = setup();
        assert!(use_case.handle(link("u1", "livret")).await.is_err());
        assert!(use_case.handle(link("inconnu", "compte")).await.is_err());
        use_case.handle(link("u1", "compte")).await.unwrap();
        assert!(use_case.handle(link("u2", "compte")).await.is_err());
        // Rattacher deux fois le même compte au même actif reste permis.
        use_case.handle(link("u1", "compte")).await.unwrap();
    }
}
