use anyhow::Context;
use asset::{Asset, AssetProvider};
use async_trait::async_trait;
use shared::AssetId;
use sqlx::{FromRow, SqlitePool};

pub struct PersistedAssets {
    pool: SqlitePool,
}

impl PersistedAssets {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct AssetRow {
    id: String,
    name: String,
    kind: String,
    currency: String,
}

impl TryFrom<AssetRow> for Asset {
    type Error = anyhow::Error;

    fn try_from(row: AssetRow) -> anyhow::Result<Self> {
        Ok(Asset {
            kind: row.kind.parse().map_err(anyhow::Error::msg)?,
            currency: row.currency.parse()?,
            id: AssetId::from(row.id),
            name: row.name,
        })
    }
}

#[async_trait]
impl AssetProvider for PersistedAssets {
    async fn read_all(&self) -> anyhow::Result<Vec<Asset>> {
        sqlx::query_as::<_, AssetRow>("SELECT id, name, kind, currency FROM assets ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .context("Impossible de lire les actifs.")?
            .into_iter()
            .map(Asset::try_from)
            .collect()
    }

    async fn read(&self, id: &AssetId) -> anyhow::Result<Option<Asset>> {
        sqlx::query_as::<_, AssetRow>("SELECT id, name, kind, currency FROM assets WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .with_context(|| format!("Impossible de lire l'actif {id}."))?
            .map(Asset::try_from)
            .transpose()
    }

    async fn write(&self, asset: &Asset) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO assets (id, name, kind, currency) VALUES (?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, kind = excluded.kind, currency = excluded.currency",
        )
        .bind(asset.id.as_str())
        .bind(&asset.name)
        .bind(asset.kind.code())
        .bind(asset.currency.code())
        .execute(&self.pool)
        .await
        .with_context(|| format!("Impossible d'écrire l'actif {}.", asset.id))?;
        Ok(())
    }

    async fn delete(&self, id: &AssetId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM assets WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer l'actif {id}."))?;
        Ok(())
    }
}
