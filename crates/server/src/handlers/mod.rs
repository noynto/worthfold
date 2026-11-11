//! Handlers HTTP : chacun appelle un seul cas d'usage et traduit son résultat en réponse.
//!
//! Deux façons d'envoyer un formulaire, une seule logique :
//! - avec htmx (en-tête `HX-Request`) : un refus répond 422 avec le message, affiché sous le formulaire ;
//!   un succès répond `HX-Trigger: worth-changed` (les fragments se rechargent) ou `HX-Redirect` ;
//! - sans JavaScript : redirection 303, avec le message en paramètre `?error=` en cas de refus.
//!
//! Un module par page (`dashboard`, `assets_page`, `asset_page`) avec ses fragments et ses formulaires,
//! `valuations` pour les formulaires de valorisation, `views` pour ce que les pages partagent,
//! et `reply` pour les réponses (rendu, formulaires, pannes, page introuvable).

mod asset_page;
mod assets_page;
mod dashboard;
mod reply;
mod valuations;
mod views;

pub use asset_page::{get_asset_fragment, get_asset_page, post_asset_delete, post_asset_update};
pub use assets_page::{get_assets_fragment, get_assets_page, post_asset};
pub use dashboard::{
    get_chart_fragment, get_dashboard, get_net_worth_chart, get_net_worth_fragment,
};
pub use valuations::{post_valuation, post_valuation_delete};

/// Toute route inconnue.
pub async fn not_found() -> axum::response::Response {
    reply::not_found(
        "Page introuvable",
        "Cette adresse ne mène à aucune page de worthfold.",
    )
}

use std::sync::Arc;

use application::UseCases;
use axum::extract::State;

pub(crate) type AppState = State<Arc<UseCases>>;

/// Événement htmx émis après toute modification du patrimoine : le total, la liste et le graphique l'écoutent.
pub(crate) const WORTH_CHANGED: &str = "worth-changed";
