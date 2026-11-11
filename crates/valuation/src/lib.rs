//! Domaine des valorisations : la valeur d'un actif à une date.
//! On n'écrase jamais une valorisation, on en ajoute une nouvelle : c'est l'historique.

use async_trait::async_trait;
use chrono::NaiveDate;
use shared::{AssetId, Money, ValuationId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valuation {
    pub id: ValuationId,
    pub asset_id: AssetId,
    pub value: Money,
    pub valued_on: NaiveDate,
}

impl Valuation {
    /// Au-delà de cet âge, une valorisation ne reflète plus la valeur actuelle de l'actif.
    pub const STALE_AFTER_DAYS: i64 = 90;

    /// Vrai si la valorisation a plus de `STALE_AFTER_DAYS` jours au jour `today` : l'actif est à mettre à jour.
    pub fn is_stale(&self, today: NaiveDate) -> bool {
        (today - self.valued_on).num_days() > Self::STALE_AFTER_DAYS
    }
}

#[async_trait]
pub trait ValuationProvider: Send + Sync {
    async fn read_all(&self) -> anyhow::Result<Vec<Valuation>>;
    async fn read(&self, id: &ValuationId) -> anyhow::Result<Option<Valuation>>;
    async fn write(&self, valuation: &Valuation) -> anyhow::Result<()>;
    async fn delete(&self, id: &ValuationId) -> anyhow::Result<()>;
    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use shared::Currency;

    fn valued_on(date: NaiveDate) -> Valuation {
        Valuation {
            id: ValuationId::generate(),
            asset_id: "a".into(),
            value: Money::new(Decimal::from(100), Currency::Eur),
            valued_on: date,
        }
    }

    #[test]
    fn une_valorisation_devient_ancienne_au_dela_de_90_jours() {
        let day = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        let valuation = valued_on(day(8, 1));
        assert!(!valuation.is_stale(day(8, 1)));
        assert!(!valuation.is_stale(day(10, 30))); // 90 jours : encore à jour
        assert!(valuation.is_stale(day(10, 31))); // 91 jours
    }
}
