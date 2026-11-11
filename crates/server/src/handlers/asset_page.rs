//! La page d'un actif (`/assets/{id}`) : en-tête, historique, modification et suppression.

use application::{UseCaseError, UseCases, delete_asset, read_asset, update_asset};
use askama::Template;
use asset::AssetKind;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::NaiveDate;
use shared::{Currency, Money};

use super::AppState;
use super::assets_page::AssetForm;
use super::reply::{Reply, not_found, render, technical_page};
use super::views::{
    ErrorParams, currency_options, date_label, freshness, iso_date, kind_options, today,
    with_flash_consumed,
};
use crate::{flash, format};

// ── GET /assets/{id} : la page d'un actif ────────────────────────────────────

/// L'en-tête d'un actif et son historique : partagé par la page et son fragment.
struct AssetDetail {
    id: String,
    name: String,
    kind_label: &'static str,
    value: String,
    negative: bool,
    valued_on: String,
    freshness: Option<&'static str>,
    history: Vec<HistoryRow>,
    // Pour modifier et supprimer (partie `#asset-manage`, rechargée après une valorisation).
    kind_code: &'static str,
    currency: &'static str,
    /// Devise et sens figés dès la première valorisation.
    currency_locked: bool,
    /// « aucune valorisation », « 1 valorisation », « 3 valorisations ».
    valuations: String,
    /// Le texte de confirmation de la suppression, qui chiffre ce qui sera perdu (règle d'UX 6).
    delete_confirm: String,
    kinds: Vec<(&'static str, &'static str)>,
    currencies: Vec<&'static str>,
}

struct HistoryRow {
    id: String,
    /// Le texte de confirmation de sa suppression : la date et le montant (règle d'UX 6).
    delete_confirm: String,
    date: String,
    value: String,
    negative: bool,
    /// Écart avec la valorisation précédente (« +1 200,00 € », « inchangée »), absent pour la première.
    variation: Option<String>,
    variation_down: bool,
}

fn asset_detail(output: &read_asset::Output, today: NaiveDate) -> AssetDetail {
    let count = output.valuations.len();
    let latest = output.valuations.first();
    // L'historique va du plus récent au plus ancien : la valorisation précédente est la suivante de la liste.
    let history = output
        .valuations
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let delta = output
                .valuations
                .get(i + 1)
                .map(|previous| v.value.amount - previous.value.amount);
            HistoryRow {
                id: v.id.to_string(),
                delete_confirm: format!(
                    "Supprimer la valorisation du {} ({}) ? C'est définitif.",
                    date_label(v.valued_on),
                    format::money(&v.value)
                ),
                date: date_label(v.valued_on),
                value: format::money(&v.value),
                negative: v.value.amount.is_sign_negative(),
                variation: delta.map(|d| {
                    if d.is_zero() {
                        "inchangée".to_owned()
                    } else {
                        let sign = if d.is_sign_positive() { "+" } else { "" };
                        format!("{sign}{}", format::money(&Money::new(d, v.value.currency)))
                    }
                }),
                variation_down: delta.is_some_and(|d| d.is_sign_negative() && !d.is_zero()),
            }
        })
        .collect();
    AssetDetail {
        id: output.asset.id.to_string(),
        name: output.asset.name.clone(),
        kind_label: output.asset.kind.label(),
        value: latest.map_or_else(|| "—".to_owned(), |v| format::money(&v.value)),
        negative: latest.is_some_and(|v| v.value.amount.is_sign_negative()),
        valued_on: latest.map_or_else(String::new, |v| date_label(v.valued_on)),
        freshness: freshness(latest, today),
        history,
        kind_code: output.asset.kind.code(),
        currency: output.asset.currency.code(),
        currency_locked: count > 0,
        valuations: format::count(count, "valorisation", "valorisations"),
        delete_confirm: match count {
            0 => format!("Supprimer « {} » ? C'est définitif.", output.asset.name),
            n => format!(
                "Supprimer « {} » et {} ? C'est définitif.",
                output.asset.name,
                format::count(n, "sa valorisation", "ses valorisations")
            ),
        },
        kinds: kind_options(),
        currencies: currency_options(),
    }
}

async fn read_asset(app: &UseCases, id: String) -> Result<read_asset::Output, Box<Response>> {
    match app
        .read_asset
        .handle(read_asset::Query {
            asset_id: id.into(),
        })
        .await
    {
        Ok(output) => Ok(output),
        Err(UseCaseError::Refusal(message)) => {
            Err(Box::new(not_found("Actif introuvable", &message)))
        }
        Err(e) => Err(Box::new(technical_page(e))),
    }
}

#[derive(Template)]
#[template(path = "asset_page.html")]
struct AssetPage {
    section: &'static str,
    detail: AssetDetail,
    /// « Montant », ou « Capital restant dû » pour un prêt.
    amount_label: String,
    today_iso: String,
    error: Option<String>,
    /// Confirmation d'une action qui a redirigé ici, affichée dans la snackbar (message flash).
    flash: Option<String>,
}

pub async fn get_asset_page(
    State(app): AppState,
    Path(id): Path<String>,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let output = match read_asset(&app, id).await {
        Ok(output) => output,
        Err(response) => return *response,
    };
    let today = today();
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(AssetPage {
        section: "assets",
        amount_label: format!(
            "{} ({})",
            if output.asset.kind.is_liability() {
                "Capital restant dû"
            } else {
                "Montant"
            },
            format::symbol(output.asset.currency)
        ),
        detail: asset_detail(&output, today),
        today_iso: iso_date(today),
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

#[derive(Template)]
#[template(path = "partials/asset_detail.html")]
struct AssetDetailFragment {
    detail: AssetDetail,
}

pub async fn get_asset_fragment(State(app): AppState, Path(id): Path<String>) -> Response {
    match read_asset(&app, id).await {
        Ok(output) => render(AssetDetailFragment {
            detail: asset_detail(&output, today()),
        }),
        Err(response) => *response,
    }
}

// ── POST /assets/{id} ────────────────────────────────────────────────────────

pub async fn post_asset_update(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<AssetForm>,
) -> Response {
    let page = format!("/assets/{id}");
    let reply = Reply::new(&headers, &page);
    let (Ok(kind), Ok(currency)) = (
        form.kind.parse::<AssetKind>(),
        form.currency.parse::<Currency>(),
    ) else {
        return reply.refusal("Le type ou la devise de l'actif est invalide.");
    };
    let command = update_asset::Command {
        asset_id: id.into(),
        name: form.name,
        kind,
        currency,
    };
    match app.update_asset.handle(command).await {
        Ok(output) => reply.redirect(&page, &format!("« {} » enregistré.", output.asset.name)),
        Err(e) => reply.error(e),
    }
}

// ── POST /assets/{id}/delete ─────────────────────────────────────────────────

pub async fn post_asset_delete(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, &format!("/assets/{id}"));
    let command = delete_asset::Command {
        asset_id: id.into(),
    };
    match app.delete_asset.handle(command).await {
        Ok(output) => reply.redirect("/assets", &format!("« {} » supprimé.", output.asset.name)),
        Err(e) => reply.error(e),
    }
}
