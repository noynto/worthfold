use std::sync::Arc;

use asset::{Asset, AssetProvider};
use shared::AssetId;
use valuation::{Valuation, ValuationProvider};

use crate::{NOT_FOUND, UseCaseError};

pub struct ReadAsset {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
}

pub struct Query {
    pub asset_id: AssetId,
}

pub struct Output {
    pub asset: Asset,
    /// L'historique de l'actif, de la plus récente à la plus ancienne valorisation.
    /// Deux saisies à la même date gardent leur ordre de saisie (la dernière saisie d'abord).
    /// Dès qu'il n'est pas vide, la devise et le sens de l'actif sont figés.
    pub valuations: Vec<Valuation>,
}

impl ReadAsset {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let asset = self
            .asset_provider
            .read(&query.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        // Le provider rend les valorisations dans l'ordre de saisie : un tri stable sur la date,
        // puis l'inversion, met la plus récente (et, à date égale, la dernière saisie) en tête.
        let mut valuations: Vec<Valuation> = self
            .valuation_provider
            .read_all()
            .await?
            .into_iter()
            .filter(|v| v.asset_id == asset.id)
            .collect();
        valuations.sort_by_key(|v| v.valued_on);
        valuations.reverse();
        Ok(Output { asset, valuations })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::AssetKind;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::{InMemoryAssets, InMemoryValuations};

    fn valuation(id: &str, amount: i64, day: u32) -> Valuation {
        Valuation {
            id: shared::ValuationId::generate(),
            asset_id: id.into(),
            value: Money::new(Decimal::from(amount), Currency::Eur),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
        }
    }

    #[tokio::test]
    async fn rend_l_historique_de_l_actif_du_plus_recent_au_plus_ancien() {
        let asset = Asset {
            id: "a".into(),
            name: "Livret".into(),
            kind: AssetKind::SavingsBook,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        // Saisies dans le désordre, deux à la même date (la seconde corrige la première), une d'un autre actif.
        let valuations = [
            valuation("a", 100, 5),
            valuation("a", 50, 1),
            valuation("autre", 999, 9),
            valuation("a", 120, 5),
        ];
        let use_case = ReadAsset {
            asset_provider: Arc::new(InMemoryAssets::with([asset])),
            valuation_provider: Arc::new(InMemoryValuations::with(valuations)),
        };

        let output = use_case
            .handle(Query {
                asset_id: "a".into(),
            })
            .await
            .unwrap();

        let amounts: Vec<_> = output.valuations.iter().map(|v| v.value.amount).collect();
        assert_eq!(
            amounts,
            [Decimal::from(120), Decimal::from(100), Decimal::from(50)]
        );
    }

    #[tokio::test]
    async fn refuse_un_actif_inconnu() {
        let use_case = ReadAsset {
            asset_provider: Arc::new(InMemoryAssets::default()),
            valuation_provider: Arc::new(InMemoryValuations::default()),
        };
        let result = use_case
            .handle(Query {
                asset_id: "x".into(),
            })
            .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }
}
