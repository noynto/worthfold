//! Assemblage : les adaptateurs de l'infrastructure branchés sur les cas d'usage.

use std::sync::Arc;

use application::{AuthPorts, UseCases};
use bank::BankAggregator;
use infrastructure::argon2_hasher::Argon2Hasher;
use infrastructure::enablebanking::EnableBanking;
use infrastructure::persistence::{
    self, PersistedAssets, PersistedAuth, PersistedBank, PersistedBudget, PersistedCategories,
    PersistedRecurrence, PersistedTransactions, PersistedValuations,
};

use crate::config::{BankConfig, Config};

/// Ce que l'assemblage rend : les cas d'usage, et le code de configuration tiré pour cette exécution.
pub struct Assembled {
    pub use_cases: UseCases,
    pub setup_code: String,
}

pub async fn build(config: &Config) -> anyhow::Result<Assembled> {
    let pool = persistence::connect(&config.database_url).await?;
    let aggregator = config.bank.as_ref().map(bank_aggregator).transpose()?;
    let setup_code = new_setup_code();
    let use_cases = UseCases::new(
        Arc::new(PersistedAssets::new(pool.clone())),
        Arc::new(PersistedValuations::new(pool.clone())),
        Arc::new(PersistedTransactions::new(pool.clone())),
        Arc::new(PersistedCategories::new(pool.clone())),
        Arc::new(PersistedBudget::new(pool.clone())),
        Arc::new(PersistedRecurrence::new(pool.clone())),
        Arc::new(PersistedBank::new(pool.clone())),
        aggregator,
        AuthPorts {
            provider: Arc::new(PersistedAuth::new(pool)),
            hasher: Arc::new(Argon2Hasher),
            setup_code: setup_code.clone(),
        },
    );
    Ok(Assembled {
        use_cases,
        setup_code,
    })
}

/// Un code à recopier depuis la console, du type `A1B2-C3D4-E5F6` (48 bits : avec les essais freinés, hors de portée).
fn new_setup_code() -> String {
    let token = shared::random_token().to_uppercase();
    format!("{}-{}-{}", &token[0..4], &token[4..8], &token[8..12])
}

/// L'agrégateur EnableBanking : une clé illisible arrête le démarrage, plutôt que de faire échouer plus tard.
fn bank_aggregator(bank: &BankConfig) -> anyhow::Result<Arc<dyn BankAggregator>> {
    Ok(Arc::new(EnableBanking::from_pem_file(
        bank.application_id.clone(),
        &bank.key_path,
        bank.redirect_url.clone(),
    )?))
}
