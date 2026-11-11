use std::sync::Arc;

use asset::{Asset, AssetKind, AssetProvider};
use shared::{AssetId, Currency};

use crate::UseCaseError;

pub struct AddAsset {
    pub asset_provider: Arc<dyn AssetProvider>,
}

pub struct Command {
    pub name: String,
    pub kind: AssetKind,
    pub currency: Currency,
}

pub struct Output {
    pub asset: Asset,
}

impl AddAsset {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        // 1. Vérification de la commande.
        let name = Asset::validate_name(&command.name).map_err(UseCaseError::Refusal)?;
        // 2. Écriture.
        let asset = Asset {
            id: AssetId::generate(),
            name,
            kind: command.kind,
            currency: command.currency,
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
}
