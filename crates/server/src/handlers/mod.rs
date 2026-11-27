//! Handlers HTTP : chacun appelle un seul cas d'usage et traduit son résultat en réponse.
//!
//! Deux façons d'envoyer un formulaire, une seule logique :
//! - avec htmx (en-tête `HX-Request`) : un refus répond 422 avec le message, affiché sous le formulaire ;
//!   un succès répond `HX-Trigger: worth-changed` (les fragments se rechargent) ou `HX-Redirect` ;
//! - sans JavaScript : redirection 303, avec le message en paramètre `?error=` en cas de refus.
//!
//! Un module par page (`dashboard`, `assets_page`, `asset_page`) avec ses fragments et ses formulaires,
//! `valuations` pour les formulaires de valorisation, `transactions` pour les opérations d'un compte, `categories` pour la page Catégories, `budget` pour la page Budget, `payments` pour la page Paiements, `forecast` pour la page Prévisions, `bank` pour la page Banque et le retour de la banque, `views` pour ce que les pages partagent,
//! et `reply` pour les réponses (rendu, formulaires, pannes, page introuvable).

mod asset_page;
mod assets_page;
mod auth;
mod bank;
mod budget;
mod categories;
mod dashboard;
mod forecast;
mod institutions;
mod payments;
mod reply;
mod settings;
mod transactions;
mod valuations;
mod views;

pub use asset_page::{get_asset_fragment, get_asset_page, post_asset_delete, post_asset_update};
pub use assets_page::{get_assets_fragment, get_assets_page, post_asset};
pub use auth::{
    get_account, get_login, get_setup, post_change_password, post_login, post_logout, post_setup,
};
pub use settings::get_settings;
pub use bank::{
    get_bank_callback, get_bank_page, get_banks_fragment, post_bank_connect, post_bank_delete,
    post_bank_link, post_bank_sync, post_bank_sync_all,
};
pub use budget::{
    get_budget_current, get_budget_page, post_envelope, post_envelope_remove, post_savings_plan,
};
pub use categories::{
    get_categories_page, post_apply_rules, post_category, post_category_delete,
    post_default_categories, post_rule, post_rule_delete,
};
pub use dashboard::{
    get_chart_fragment, get_dashboard, get_net_worth_chart, get_net_worth_fragment,
};
pub use forecast::get_forecast_page;
pub use institutions::{
    get_banks_page, post_institution, post_institution_delete, post_institution_update,
};
pub use payments::{
    get_payments_page, post_occurrence, post_payment, post_payment_delete, post_payment_step,
    post_suggestion,
};
pub use transactions::{
    get_transactions_page, post_transaction_category, post_transactions_import,
};
pub use valuations::{post_valuation, post_valuation_delete};
pub(crate) use views::now;

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
