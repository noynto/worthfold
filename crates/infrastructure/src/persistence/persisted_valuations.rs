use anyhow::Context;
use async_trait::async_trait;
use chrono::NaiveDate;
use shared::{AssetId, Money, ValuationId};
use sqlx::{FromRow, SqlitePool};
use valuation::{Valuation, ValuationProvider};

pub struct PersistedValuations {
    pool: SqlitePool,
}

impl PersistedValuations {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct ValuationRow {
    id: String,
    asset_id: String,
    amount: String,
    currency: String,
    valued_on: String,
}

impl TryFrom<ValuationRow> for Valuation {
    type Error = anyhow::Error;

    fn try_from(row: ValuationRow) -> anyhow::Result<Self> {
        Ok(Valuation {
            id: ValuationId::from(row.id),
            asset_id: AssetId::from(row.asset_id),
            value: Money::new(row.amount.parse()?, row.currency.parse()?),
            valued_on: NaiveDate::parse_from_str(&row.valued_on, "%Y-%m-%d")?,
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
            "SELECT id, asset_id, amount, currency, valued_on FROM valuations WHERE id = ?",
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
            "INSERT INTO valuations (id, asset_id, amount, currency, valued_on) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(valuation.id.as_str())
        .bind(valuation.asset_id.as_str())
        .bind(valuation.value.amount.to_string())
        .bind(valuation.value.currency.code())
        .bind(valuation.valued_on.format("%Y-%m-%d").to_string())
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
        sqlx::query("DELETE FROM valuations WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer la valorisation {id}."))?;
        Ok(())
    }

    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM valuations WHERE asset_id = ?")
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
    use crate::persistence::{PersistedAssets, connect};
    use asset::{Asset, AssetKind, AssetProvider};
    use rust_decimal::Decimal;
    use shared::Currency;

    /// Une base par test, dans un fichier : `sqlite::memory:` donnerait une base vide par connexion du pool.
    async fn database() -> (SqlitePool, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("worthfold-test-{}.db", AssetId::generate()));
        let pool = connect(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        (pool, path)
    }

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
        let (pool, path) = database().await;
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
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn la_migration_garde_l_ordre_des_anciennes_valorisations() {
        // Reproduit une base d'avant la migration 0002 : identifiants entiers, ordre de saisie 2 puis 10.
        let path = std::env::temp_dir().join(format!("worthfold-test-{}.db", AssetId::generate()));
        let url = format!("sqlite://{}", path.display());
        {
            use sqlx::sqlite::SqliteConnectOptions;
            use std::str::FromStr;
            let options = SqliteConnectOptions::from_str(&url)
                .unwrap()
                .create_if_missing(true);
            let old = SqlitePool::connect_with(options).await.unwrap();
            let mut migrator = sqlx::migrate!("./migrations");
            migrator.set_ignore_missing(true);
            let first_only: Vec<_> = migrator
                .migrations
                .iter()
                .filter(|m| m.version == 1)
                .cloned()
                .collect();
            migrator.migrations = std::borrow::Cow::Owned(first_only);
            migrator.run(&old).await.unwrap();
            sqlx::query("INSERT INTO assets VALUES ('a', 'Livret', 'savings_book', 'EUR')")
                .execute(&old)
                .await
                .unwrap();
            sqlx::query("INSERT INTO valuations (id, asset_id, amount, currency, valued_on) VALUES (2, 'a', '10', 'EUR', '2026-11-09'), (10, 'a', '20', 'EUR', '2026-11-09')")
                .execute(&old)
                .await
                .unwrap();
            old.close().await;
        }

        let pool = connect(&url).await.unwrap();
        let valuations = PersistedValuations::new(pool);
        let amounts: Vec<_> = valuations
            .read_all()
            .await
            .unwrap()
            .into_iter()
            .map(|v| v.value.amount)
            .collect();
        assert_eq!(amounts, [Decimal::from(10), Decimal::from(20)]);
        let _ = std::fs::remove_file(path);
    }
}
