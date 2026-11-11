//! La page Actifs (`/assets`), sa liste (fragment) et l'ajout d'un actif.

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
    kinds: Vec<(&'static str, &'static str)>,
    currencies: Vec<&'static str>,
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
        kinds: kind_options(),
        currencies: currency_options(),
    });
    with_flash_consumed(page, had_flash)
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
}

pub async fn post_asset(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<AssetForm>,
) -> Response {
    let reply = Reply::new(&headers, "/assets");
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
    };
    match app.add_asset.handle(command).await {
        Ok(output) => reply.worth_changed(&format!(
            "« {} » ajouté : saisis sa première valeur dans la liste.",
            output.asset.name
        )),
        Err(e) => reply.error(e),
    }
}
