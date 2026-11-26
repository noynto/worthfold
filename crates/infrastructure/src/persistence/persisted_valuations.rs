use anyhow::Context;
use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{AssetId, Money, ValuationId};
use sqlx::{FromRow, PgPool};
use valuation::{Valuation, ValuationProvider};

pub struct PersistedValuations {
    pool: PgPool,
}

impl PersistedValuations {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct ValuationRow {
    id: String,
    asset_id: String,
    amount: Decimal,
    currency: String,
    valued_on: NaiveDate,
}

impl TryFrom<ValuationRow> for Valuation {
    type Error = anyhow::Error;

    fn try_from(row: ValuationRow) -> anyhow::Result<Self> {
        Ok(Valuation {
            id: ValuationId::from(row.id),
            asset_id: AssetId::from(row.asset_id),
            value: Money::new(row.amount, row.currency.parse()?),
            valued_on: row.valued_on,
        })
    }
}

#[async_trait]
impl ValuationProvider for PersistedValuations {
    /// Par date, puis par ordre de saisie (les identifiants UUID v7 sont triés dans le temps).
    async fn read_all(&self) -> anyhow::Result<Vec<Valuation>> {
        sqlx::query_as::<_, ValuationRow>(
            "SELECT id, asset_id, amount, currency, valued_on FROM valuations ORDER BY valued_on, id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les valorisations.")?
        .into_iter()
        .map(Valuation::try_from)
        .collect()
    }

    async fn read(&self, id: &ValuationId) -> anyhow::Result<Option<Valuation>> {
        sqlx::query_as::<_, ValuationRow>(
            "SELECT id, asset_id, amount, currency, valued_on FROM valuations WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Impossible de lire la valorisation {id}."))?
        .map(Valuation::try_from)
        .transpose()
    }

    async fn write(&self, valuation: &Valuation) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO valuations (id, asset_id, amount, currency, valued_on) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(valuation.id.as_str())
        .bind(valuation.asset_id.as_str())
        .bind(valuation.value.amount)
        .bind(valuation.value.currency.code())
        .bind(valuation.valued_on)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!(
                "Impossible d'écrire la valorisation de l'actif {}.",
                valuation.asset_id
            )
        })?;
        Ok(())
    }

    async fn delete(&self, id: &ValuationId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM valuations WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer la valorisation {id}."))?;
        Ok(())
    }

    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM valuations WHERE asset_id = $1")
            .bind(asset_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de supprimer les valorisations de l'actif {asset_id}.")
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedAssets;
    use crate::persistence::testdb::database;
    use asset::{Asset, AssetKind, AssetProvider};
    use rust_decimal::Decimal;
    use shared::Currency;

    fn valuation(asset: &Asset, amount: i64) -> Valuation {
        Valuation {
            id: ValuationId::generate(),
            asset_id: asset.id.clone(),
            value: Money::new(Decimal::new(amount, 2), Currency::Eur),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
        }
    }

    #[tokio::test]
    async fn ecrit_relit_puis_supprime_une_valorisation() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let valuations = PersistedValuations::new(pool);
        let asset = Asset {
            id: AssetId::generate(),
            name: "Livret A".into(),
            kind: AssetKind::SavingsBook,
            currency: Currency::Eur,
        };
        assets.write(&asset).await.unwrap();
        let first = valuation(&asset, 123_456);
        let second = valuation(&asset, 130_000);
        valuations.write(&first).await.unwrap();
        valuations.write(&second).await.unwrap();

        assert_eq!(assets.read(&asset.id).await.unwrap(), Some(asset));
        // Même date : l'ordre de saisie est conservé.
        assert_eq!(
            valuations.read_all().await.unwrap(),
            vec![first.clone(), second.clone()]
        );
        assert_eq!(
            valuations.read(&second.id).await.unwrap(),
            Some(second.clone())
        );

        valuations.delete(&first.id).await.unwrap();
        assert_eq!(valuations.read_all().await.unwrap(), vec![second]);
        assert_eq!(valuations.read(&first.id).await.unwrap(), None);
        db.drop_db().await;
    }
}
