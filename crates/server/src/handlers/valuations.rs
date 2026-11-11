//! Enregistrer et supprimer une valorisation, depuis la page d'un actif.

use application::{delete_valuation, record_valuation};
use axum::Form;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;

use super::AppState;
use super::reply::Reply;
use super::views::{date_label, today};
use crate::format;

// ── POST /assets/{id}/valuations ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ValuationForm {
    amount: String,
    valued_on: String,
}

pub async fn post_valuation(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<ValuationForm>,
) -> Response {
    let reply = Reply::new(&headers, &format!("/assets/{id}"));
    // Accepte « 1 234,56 » comme « 1234.56 ».
    let amount = form.amount.replace([' ', '\u{a0}'], "").replace(',', ".");
    let Ok(amount) = amount.parse::<Decimal>() else {
        return reply.refusal(&format!(
            "Le montant « {} » n'est pas un nombre : saisis par exemple 1 234,56.",
            form.amount
        ));
    };
    let Ok(valued_on) = NaiveDate::parse_from_str(&form.valued_on, "%Y-%m-%d") else {
        return reply.refusal("La date de valorisation est invalide.");
    };
    let command = record_valuation::Command {
        asset_id: id.into(),
        amount,
        valued_on,
        today: today(),
    };
    match app.record_valuation.handle(command).await {
        Ok(output) => reply.worth_changed(&format!(
            "Valorisation de {} enregistrée.",
            format::money(&output.valuation.value)
        )),
        Err(e) => reply.error(e),
    }
}

// ── POST /assets/{id}/valuations/{valuation_id}/delete ───────────────────────

pub async fn post_valuation_delete(
    State(app): AppState,
    Path((id, valuation_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, &format!("/assets/{id}"));
    let command = delete_valuation::Command {
        asset_id: id.into(),
        valuation_id: valuation_id.into(),
    };
    match app.delete_valuation.handle(command).await {
        Ok(output) => reply.worth_changed(&format!(
            "Valorisation du {} ({}) supprimée.",
            date_label(output.valuation.valued_on),
            format::money(&output.valuation.value)
        )),
        Err(e) => reply.error(e),
    }
}
