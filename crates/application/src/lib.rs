//! Cas d'usage : un module par action, avec sa `Command` (ou `Query`), son `Output`
//! et une méthode `handle`. Seuls les cas d'usage parlent aux providers.

pub mod add_asset;
pub mod delete_asset;
pub mod delete_valuation;
pub mod read_asset;
pub mod read_net_worth;
pub mod read_net_worth_history;
pub mod record_valuation;
pub mod update_asset;

use std::sync::Arc;

use asset::AssetProvider;
use valuation::ValuationProvider;

/// Tous les cas d'usage, assemblés par `bootstrap` avec les adaptateurs de l'infrastructure, puis donnés
/// aux points d'entrée (serveur web, tâches). Ne contient rien de technique : seulement des cas d'usage.
pub struct UseCases {
    pub add_asset: add_asset::AddAsset,
    pub read_asset: read_asset::ReadAsset,
    pub update_asset: update_asset::UpdateAsset,
    pub delete_asset: delete_asset::DeleteAsset,
    pub record_valuation: record_valuation::RecordValuation,
    pub delete_valuation: delete_valuation::DeleteValuation,
    pub read_net_worth: read_net_worth::ReadNetWorth,
    pub read_net_worth_history: read_net_worth_history::ReadNetWorthHistory,
}

impl UseCases {
    /// Branche chaque cas d'usage sur les providers dont il a besoin : les adaptateurs SQLite en
    /// production (`bootstrap`), ceux en mémoire dans les tests.
    pub fn new(assets: Arc<dyn AssetProvider>, valuations: Arc<dyn ValuationProvider>) -> Self {
        Self {
            add_asset: add_asset::AddAsset {
                asset_provider: assets.clone(),
            },
            read_asset: read_asset::ReadAsset {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            update_asset: update_asset::UpdateAsset {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            delete_asset: delete_asset::DeleteAsset {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            record_valuation: record_valuation::RecordValuation {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            delete_valuation: delete_valuation::DeleteValuation {
                valuation_provider: valuations.clone(),
            },
            read_net_worth: read_net_worth::ReadNetWorth {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            read_net_worth_history: read_net_worth_history::ReadNetWorthHistory {
                asset_provider: assets,
                valuation_provider: valuations,
            },
        }
    }
}

/// Erreur commune des cas d'usage.
#[derive(Debug, thiserror::Error)]
pub enum UseCaseError {
    /// Refus d'une règle métier : le message, en français, peut être montré tel quel.
    #[error("{0}")]
    Refusal(String),
    /// Panne technique (base, réseau…) : journalisée, jamais montrée en détail.
    #[error(transparent)]
    Technical(#[from] anyhow::Error),
}

/// Refus commun quand l'actif visé n'existe pas (lien périmé, actif supprimé entre-temps).
/// Jamais d'identifiant technique dans un message montré à l'utilisateur.
pub(crate) const NOT_FOUND: &str = "Cet actif n'existe plus : il a peut-être été supprimé.";

impl UseCaseError {
    pub fn refusal(message: impl Into<String>) -> Self {
        Self::Refusal(message.into())
    }
}
