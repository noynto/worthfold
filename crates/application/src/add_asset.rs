use std::sync::Arc;

use asset::{Asset, AssetKind, AssetProvider};
use shared::{AssetId, Currency};

use crate::UseCaseError;
use crate::bank_details::bank_details;

pub struct AddAsset {
    pub asset_provider: Arc<dyn AssetProvider>,
}

pub struct Command {
    pub name: String,
    pub kind: AssetKind,
    pub currency: Currency,
    /// Banque et numéro de compte, tels que saisis : retenus pour un compte bancaire seulement.
    pub bank_name: String,
    pub account_number: String,
}

pub struct Output {
    pub asset: Asset,
}

impl AddAsset {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        // 1. Vérification de la commande.
        let name = Asset::validate_name(&command.name).map_err(UseCaseError::Refusal)?;
        let (bank_name, account_number) =
            bank_details(command.kind, &command.bank_name, &command.account_number)?;
        // 2. Écriture.
        let asset = Asset {
            id: AssetId::generate(),
            name,
            kind: command.kind,
            currency: command.currency,
            bank_name,
            account_number,
        };
        self.asset_provider.write(&asset).await?;
        Ok(Output { asset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::InMemoryAssets;

    fn command(name: &str) -> Command {
        Command {
            name: name.into(),
            kind: AssetKind::SavingsBook,
            currency: Currency::Eur,
            bank_name: String::new(),
            account_number: String::new(),
        }
    }

    #[tokio::test]
    async fn ajoute_un_actif() {
        let assets = Arc::new(InMemoryAssets::default());
        let output = AddAsset {
            asset_provider: assets.clone(),
        }
        .handle(command("  Livret A "))
        .await
        .unwrap();

        assert_eq!(output.asset.name, "Livret A");
        assert_eq!(
            assets.read(&output.asset.id).await.unwrap(),
            Some(output.asset)
        );
    }

    #[tokio::test]
    async fn refuse_un_nom_vide() {
        let result = AddAsset {
            asset_provider: Arc::new(InMemoryAssets::default()),
        }
        .handle(command("   "))
        .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }

    #[tokio::test]
    async fn un_compte_bancaire_garde_sa_banque_et_son_numero_normalise() {
        let assets = Arc::new(InMemoryAssets::default());
        let output = AddAsset {
            asset_provider: assets.clone(),
        }
        .handle(Command {
            kind: AssetKind::BankAccount,
            bank_name: " Ma banque ".into(),
            account_number: "fr14 2004 1010 0505 0001 3m02 606".into(),
            ..command("Compte courant")
        })
        .await
        .unwrap();
        assert_eq!(output.asset.bank_name.as_deref(), Some("Ma banque"));
        assert_eq!(
            output.asset.account_number.as_deref(),
            Some("FR1420041010050500013M02606")
        );
    }

    #[tokio::test]
    async fn un_autre_type_n_a_ni_banque_ni_numero_meme_saisis() {
        let output = AddAsset {
            asset_provider: Arc::new(InMemoryAssets::default()),
        }
        .handle(Command {
            bank_name: "Ma banque".into(),
            account_number: "FR1420041010050500013M02606".into(),
            ..command("Livret A")
        })
        .await
        .unwrap();
        assert_eq!(
            (output.asset.bank_name, output.asset.account_number),
            (None, None)
        );
    }

    #[tokio::test]
    async fn refuse_un_iban_mal_recopie() {
        let result = AddAsset {
            asset_provider: Arc::new(InMemoryAssets::default()),
        }
        .handle(Command {
            kind: AssetKind::BankAccount,
            account_number: "FR1420041010050500013M02607".into(),
            ..command("Compte courant")
        })
        .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(m)) if m.contains("IBAN")));
    }
}
