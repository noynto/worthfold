use std::sync::Arc;

use asset::{Asset, AssetProvider};
use chrono::NaiveDate;
use institution::InstitutionProvider;
use shared::{Currency, CurrencyMismatch, Money};
use valuation::{Valuation, ValuationProvider};

use crate::UseCaseError;

pub struct ReadNetWorth {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub institution_provider: Arc<dyn InstitutionProvider>,
}

pub struct Query {
    pub at: NaiveDate,
    pub currency: Currency,
}

pub struct Output {
    /// Le total, ou la raison pour laquelle il n'est pas calculable (des devises mélangées).
    /// Ce n'est pas un refus : la liste des actifs reste utile, pour corriger justement.
    pub total: Result<Money, CurrencyMismatch>,
    /// Avoirs et dettes, avec la même réserve que le total.
    pub breakdown: Result<service::Breakdown, CurrencyMismatch>,
    /// Chaque actif avec sa dernière valorisation connue (`None` si jamais valorisé).
    pub lines: Vec<Line>,
    /// Les noms des banques que l'on peut donner à un compte bancaire.
    pub bank_names: Vec<String>,
}

pub struct Line {
    pub asset: Asset,
    pub latest: Option<Valuation>,
}

impl ReadNetWorth {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let assets = self.asset_provider.read_all().await?;
        let valuations = self.valuation_provider.read_all().await?;

        let total = service::net_worth(&assets, &valuations, query.at, query.currency);
        let breakdown = service::breakdown(&assets, &valuations, query.at, query.currency);
        let lines = assets
            .into_iter()
            .map(|asset| {
                let latest = service::latest_valuation(&asset, &valuations, query.at).cloned();
                Line { asset, latest }
            })
            .collect();

        let bank_names = self
            .institution_provider
            .read_all()
            .await?
            .into_iter()
            .map(|i| i.name)
            .collect();
        Ok(Output {
            total,
            breakdown,
            lines,
            bank_names,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::AssetKind;
    use rust_decimal::Decimal;
    use testing::{InMemoryAssets, InMemoryValuations};

    #[tokio::test]
    async fn calcule_le_patrimoine_net() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let assets = [
            Asset {
                id: "livret".into(),
                name: "Livret A".into(),
                kind: AssetKind::SavingsBook,
                currency: Currency::Eur,
                bank_name: None,
                account_number: None,
            },
            Asset {
                id: "pea".into(),
                name: "PEA".into(),
                kind: AssetKind::Securities,
                currency: Currency::Eur,
                bank_name: None,
                account_number: None,
            },
        ];
        let valuations = [
            Valuation {
                id: shared::ValuationId::generate(),
                asset_id: "livret".into(),
                value: Money::new(Decimal::from(5000), Currency::Eur),
                valued_on: day,
            },
            Valuation {
                id: shared::ValuationId::generate(),
                asset_id: "pea".into(),
                value: Money::new(Decimal::from(12000), Currency::Eur),
                valued_on: day,
            },
        ];
        let use_case = ReadNetWorth {
            asset_provider: Arc::new(InMemoryAssets::with(assets)),
            valuation_provider: Arc::new(InMemoryValuations::with(valuations)),
            institution_provider: Arc::new(testing::InMemoryInstitutions::default()),
        };

        let output = use_case
            .handle(Query {
                at: day,
                currency: Currency::Eur,
            })
            .await
            .unwrap();

        assert_eq!(
            output.total,
            Ok(Money::new(Decimal::from(17000), Currency::Eur))
        );
        assert_eq!(output.lines.len(), 2);
    }
}
