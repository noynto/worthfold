use std::sync::Arc;

use asset::AssetProvider;
use chrono::NaiveDate;
use shared::{Currency, Money};
use valuation::ValuationProvider;

use crate::UseCaseError;

pub struct ReadNetWorthHistory {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
}

pub struct Query {
    pub until: NaiveDate,
    pub currency: Currency,
}

pub struct Output {
    /// Points triés par date ; vide tant qu'aucune valorisation n'a été saisie.
    pub points: Vec<(NaiveDate, Money)>,
}

impl ReadNetWorthHistory {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let assets = self.asset_provider.read_all().await?;
        let valuations = self.valuation_provider.read_all().await?;
        let points = service::net_worth_history(&assets, &valuations, query.until, query.currency)
            .map_err(|e| UseCaseError::refusal(e.to_string()))?;
        Ok(Output { points })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use rust_decimal::Decimal;
    use testing::{InMemoryAssets, InMemoryValuations};
    use valuation::Valuation;

    #[tokio::test]
    async fn prolonge_la_courbe_jusqu_a_la_date_demandee() {
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let use_case = ReadNetWorthHistory {
            asset_provider: Arc::new(InMemoryAssets::with([Asset {
                id: "a".into(),
                name: "Livret".into(),
                kind: AssetKind::SavingsBook,
                currency: Currency::Eur,
            }])),
            valuation_provider: Arc::new(InMemoryValuations::with([Valuation {
                id: shared::ValuationId::generate(),
                asset_id: "a".into(),
                value: Money::new(Decimal::from(100), Currency::Eur),
                valued_on: day(1),
            }])),
        };

        let output = use_case
            .handle(Query {
                until: day(9),
                currency: Currency::Eur,
            })
            .await
            .unwrap();

        let euros = Money::new(Decimal::from(100), Currency::Eur);
        assert_eq!(output.points, [(day(1), euros), (day(9), euros)]);
    }
}
