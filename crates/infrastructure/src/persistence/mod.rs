mod persisted_assets;
mod persisted_valuations;

use std::str::FromStr;

use anyhow::Context;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;

pub use persisted_assets::PersistedAssets;
pub use persisted_valuations::PersistedValuations;

/// Ouvre la base SQLite (créée si absente) et applique les migrations.
pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)
        .with_context(|| format!("Adresse de base de données invalide : {database_url}"))?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(options)
        .await
        .context("Impossible d'ouvrir la base de données.")?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Impossible d'appliquer les migrations.")?;
    Ok(pool)
}
