//! La page Actifs (`/assets`), sa liste (fragment), et la page d'ajout d'un actif (`/assets/new`).

use application::add_asset;
use askama::Template;
use asset::AssetKind;
use axum::Form;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;
use shared::Currency;

use super::AppState;
use super::reply::{Reply, render, technical_fragment, technical_page};
use super::views::{
    ErrorParams, LineView, currency_options, kind_options, line_views, net_worth_query, today,
    with_flash_consumed,
};
use crate::flash;

// ── GET /assets : gestion des actifs ─────────────────────────────────────────

#[derive(Template)]
#[template(path = "assets_page.html")]
struct AssetsPage {
    section: &'static str,
    error: Option<String>,
    /// Confirmation d'une action qui a redirigé ici, affichée dans la snackbar (message flash).
    flash: Option<String>,
    lines: Vec<LineView>,
}

pub async fn get_assets_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let today = today();
    let output = match app.read_net_worth.handle(net_worth_query(today)).await {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(AssetsPage {
        section: "assets",
        error: params.error,
        flash,
        lines: line_views(output.lines, today),
    });
    with_flash_consumed(page, had_flash)
}

// ── GET /assets/new : ajouter un actif ───────────────────────────────────────

#[derive(Template)]
#[template(path = "asset_new_page.html")]
struct AssetNewPage {
    section: &'static str,
    error: Option<String>,
    kinds: Vec<(&'static str, &'static str)>,
    currencies: Vec<&'static str>,
    /// Les banques que l'on peut donner à un compte bancaire.
    bank_names: Vec<String>,
}

pub async fn get_asset_new_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
) -> Response {
    // Seules les banques servent ici : la lecture du patrimoine les fournit déjà.
    let output = match app.read_net_worth.handle(net_worth_query(today())).await {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    render(AssetNewPage {
        section: "assets",
        error: params.error,
        kinds: kind_options(),
        currencies: currency_options(),
        bank_names: output.bank_names,
    })
}

#[derive(Template)]
#[template(path = "partials/assets.html")]
struct AssetsFragment {
    lines: Vec<LineView>,
}

pub async fn get_assets_fragment(State(app): AppState) -> Response {
    let today = today();
    match app.read_net_worth.handle(net_worth_query(today)).await {
        Ok(output) => render(AssetsFragment {
            lines: line_views(output.lines, today),
        }),
        Err(e) => technical_fragment(e),
    }
}

// ── POST /assets ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
/// Le formulaire d'un actif : à l'ajout (ici) comme à la modification (page de l'actif).
pub struct AssetForm {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) currency: String,
    /// Banque et numéro : pour un compte bancaire (ignorés pour les autres types).
    #[serde(default)]
    pub(crate) bank_name: String,
    #[serde(default)]
    pub(crate) account_number: String,
}

pub async fn post_asset(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<AssetForm>,
) -> Response {
    let reply = Reply::new(&headers, "/assets/new");
    let (Ok(kind), Ok(currency)) = (
        form.kind.parse::<AssetKind>(),
        form.currency.parse::<Currency>(),
    ) else {
        return reply.refusal("Le type ou la devise de l'actif est invalide.");
    };
    let command = add_asset::Command {
        name: form.name,
        kind,
        currency,
        bank_name: form.bank_name,
        account_number: form.account_number,
    };
    match app.add_asset.handle(command).await {
        // Sur la page du nouvel actif, où l'on saisit sa première valeur.
        Ok(output) => reply.redirect(
            &format!("/assets/{}", output.asset.id),
            &format!(
                "« {} » ajouté : saisis sa première valeur.",
                output.asset.name
            ),
        ),
        Err(e) => reply.error(e),
    }
}
