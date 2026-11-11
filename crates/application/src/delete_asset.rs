use std::sync::Arc;

use asset::{Asset, AssetProvider};
use shared::AssetId;
use valuation::ValuationProvider;

use crate::{NOT_FOUND, UseCaseError};

pub struct DeleteAsset {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
}

pub struct Command {
    pub asset_id: AssetId,
}

pub struct Output {
    /// L'actif tel qu'il était avant sa suppression, pour confirmer ce qui a été supprimé.
    pub asset: Asset,
}

impl DeleteAsset {
    /// Supprime l'actif et tout son historique de valorisations.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let asset = self
            .asset_provider
            .read(&command.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        // L'historique d'abord : la base refuse une valorisation sans actif.
        // Limite connue : les deux suppressions ne sont pas dans une même transaction. Si la seconde
        // échoue, l'actif reste sans historique. Accepté pour l'instant (SQLite local, un seul utilisateur) ;
        // à terme, une unité de travail transactionnelle dans les providers (docs/tech/architecture.md).
        self.valuation_provider.delete_of_asset(&asset.id).await?;
        self.asset_provider.delete(&asset.id).await?;
        Ok(Output { asset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::{InMemoryAssets, InMemoryValuations};
    use valuation::Valuation;

    #[tokio::test]
    async fn supprime_l_actif_et_son_historique_sans_toucher_aux_autres() {
        let asset = |id: &str| Asset {
            id: id.into(),
            name: id.into(),
            kind: AssetKind::Crypto,
            currency: Currency::Eur,
        };
        let valuation = |id: &str| Valuation {
            id: shared::ValuationId::generate(),
            asset_id: id.into(),
            value: Money::new(Decimal::ONE, Currency::Eur),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
        };
        let assets = Arc::new(InMemoryAssets::with([asset("a"), asset("b")]));
        let valuations = Arc::new(InMemoryValuations::with([valuation("a"), valuation("b")]));

        DeleteAsset {
            asset_provider: assets.clone(),
            valuation_provider: valuations.clone(),
        }
        .handle(Command {
            asset_id: "a".into(),
        })
        .await
        .unwrap();

        assert_eq!(assets.read_all().await.unwrap(), vec![asset("b")]);
        let remaining: Vec<_> = valuations
            .read_all()
            .await
            .unwrap()
            .into_iter()
            .map(|v| v.asset_id)
            .collect();
        assert_eq!(remaining, vec![AssetId::from("b")]);
    }

    #[tokio::test]
    async fn refuse_un_actif_inconnu() {
        let use_case = DeleteAsset {
            asset_provider: Arc::new(InMemoryAssets::default()),
            valuation_provider: Arc::new(InMemoryValuations::default()),
        };
        assert!(matches!(
            use_case
                .handle(Command {
                    asset_id: "x".into()
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
