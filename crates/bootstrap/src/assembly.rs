//! Assemblage : les adaptateurs de l'infrastructure branchés sur les cas d'usage.

use std::sync::Arc;

use application::UseCases;
use infrastructure::persistence::{self, PersistedAssets, PersistedValuations};

use crate::config::Config;

pub async fn build(config: &Config) -> anyhow::Result<UseCases> {
    let pool = persistence::connect(&config.database_url).await?;
    Ok(UseCases::new(
        Arc::new(PersistedAssets::new(pool.clone())),
        Arc::new(PersistedValuations::new(pool)),
    ))
}
