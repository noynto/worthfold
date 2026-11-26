//! La page Banque (`/bank`) : les consentements donnés, leurs comptes à rattacher, la lecture des
//! opérations, et le retour de la banque (`/bank/callback`). Les formulaires marchent sans JavaScript.

use application::{
    UseCases, complete_bank_connection, delete_bank_connection, link_bank_account, list_banks,
    read_bank_connections, start_bank_connection, sync_bank_accounts,
};
use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::AppState;
use super::reply::{Reply, render, technical_page};
use super::views::{ErrorParams, today, with_flash_consumed};
use crate::flash;

const PAGE: &str = "/bank";

struct AccountView {
    uid: String,
    label: String,
    /// Les comptes de worthfold proposés : identifiant, nom, et s'il est celui qui est rattaché.
    choices: Vec<(String, String, bool)>,
    linked: bool,
    synced: String,
}

struct ConnectionView {
    id: String,
    title: String,
    validity: String,
    /// `ok`, `soon` ou `expired` : choisit la couleur du message.
    state: &'static str,
    accounts: Vec<AccountView>,
    delete_confirm: String,
}

#[derive(Template)]
#[template(path = "bank_page.html")]
struct BankPage {
    section: &'static str,
    configured: bool,
    connections: Vec<ConnectionView>,
    has_linked: bool,
    error: Option<String>,
    flash: Option<String>,
}

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

pub async fn get_bank_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let output = match app
        .read_bank_connections
        .handle(read_bank_connections::Query { today: today() })
        .await
    {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let mut has_linked = false;
    let connections = output
        .connections
        .iter()
        .map(|view| {
            let c = &view.connection;
            let (state, validity) = match view.days_left {
                d if d < 0 => (
                    "expired",
                    format!(
                        "Consentement expiré depuis le {} : renouvelle-le pour relire les opérations.",
                        date(c.valid_until)
                    ),
                ),
                0 => ("soon", "Le consentement expire aujourd'hui.".to_owned()),
                d => (
                    if view.renew_soon { "soon" } else { "ok" },
                    format!(
                        "Consentement valable jusqu'au {} ({} jour{}).",
                        date(c.valid_until),
                        d,
                        if d > 1 { "s" } else { "" }
                    ),
                ),
            };
            ConnectionView {
                id: c.id.to_string(),
                title: format!("{} ({})", c.bank_name, c.country),
                validity,
                state,
                accounts: view
                    .accounts
                    .iter()
                    .map(|a| {
                        has_linked |= a.asset_id.is_some();
                        AccountView {
                            uid: a.uid.clone(),
                            label: a.label.clone(),
                            linked: a.asset_id.is_some(),
                            choices: output
                                .assets
                                .iter()
                                .map(|asset| {
                                    (
                                        asset.id.to_string(),
                                        asset.name.clone(),
                                        a.asset_id.as_ref() == Some(&asset.id),
                                    )
                                })
                                .collect(),
                            synced: a.last_synced_on.map_or_else(
                                || "jamais lu".to_owned(),
                                |d| format!("lu le {}", date(d)),
                            ),
                        }
                    })
                    .collect(),
                delete_confirm: format!(
                    "Retirer la connexion à {} ? Les opérations déjà lues restent dans tes comptes.",
                    c.bank_name
                ),
            }
        })
        .collect();
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(BankPage {
        section: "bank",
        configured: output.configured,
        connections,
        has_linked,
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

// ── POST /bank/connect ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ConnectForm {
    bank_name: String,
    country: String,
}

pub async fn post_bank_connect(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<ConnectForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .start_bank_connection
        .handle(start_bank_connection::Command {
            bank_name: form.bank_name,
            country: form.country,
            today: today(),
        })
        .await
    {
        // Vers la banque : une redirection pleine page, pas de message flash à afficher au passage.
        Ok(output) => Redirect::to(&output.url).into_response(),
        Err(e) => reply.error(e),
    }
}

// ── GET /bank/callback ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn get_bank_callback(
    State(app): AppState,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    if let Some(error) = params.error {
        // Le texte vient de la banque : on n'en montre rien, seulement la raison connue.
        let message = if error == "access_denied" {
            "Tu as refusé le consentement chez ta banque."
        } else {
            "La banque a interrompu la connexion."
        };
        return reply.refusal(message);
    }
    let (Some(code), Some(state)) = (params.code, params.state) else {
        return reply.refusal("Le retour de la banque est incomplet : recommence la connexion.");
    };
    match app
        .complete_bank_connection
        .handle(complete_bank_connection::Command {
            code,
            state,
            today: today(),
        })
        .await
    {
        Ok(output) => {
            let hint = if output.renewed > 0 {
                "Consentement renouvelé."
            } else {
                "Rattache maintenant chaque compte à l'un de tes comptes."
            };
            reply.redirect(
                PAGE,
                &format!("Connecté à {}. {hint}", output.connection.bank_name),
            )
        }
        Err(e) => reply.error(e),
    }
}

// ── POST /bank/accounts/{uid}/link ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct LinkForm {
    /// Vide pour détacher.
    asset_id: String,
}

pub async fn post_bank_link(
    State(app): AppState,
    Path(uid): Path<String>,
    headers: HeaderMap,
    Form(form): Form<LinkForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let asset_id = Some(form.asset_id.trim())
        .filter(|id| !id.is_empty())
        .map(Into::into);
    let detached = asset_id.is_none();
    match app
        .link_bank_account
        .handle(link_bank_account::Command { uid, asset_id })
        .await
    {
        Ok(()) => reply.redirect(
            PAGE,
            if detached {
                "Compte détaché."
            } else {
                "Compte rattaché. Lance la lecture pour récupérer ses opérations."
            },
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /bank/sync ──────────────────────────────────────────────────────────

pub async fn post_bank_sync(State(app): AppState, headers: HeaderMap) -> Response {
    sync(&app, &headers, false).await
}

/// Relit les 90 derniers jours : rattrape une opération apparue chez la banque avec une date ancienne.
pub async fn post_bank_sync_all(State(app): AppState, headers: HeaderMap) -> Response {
    sync(&app, &headers, true).await
}

async fn sync(app: &UseCases, headers: &HeaderMap, full: bool) -> Response {
    let reply = Reply::new(headers, PAGE);
    match app
        .sync_bank_accounts
        .handle(sync_bank_accounts::Command {
            today: today(),
            full,
        })
        .await
    {
        Ok(output) => {
            let mut parts = vec![format!(
                "{} nouvelle{} opération{} lue{}.",
                output.imported(),
                if output.imported() > 1 { "s" } else { "" },
                if output.imported() > 1 { "s" } else { "" },
                if output.imported() > 1 { "s" } else { "" },
            )];
            let pending: usize = output.accounts.iter().map(|a| a.pending).sum();
            if pending > 0 {
                parts.push(format!(
                    "{pending} opération{} en attente chez la banque.",
                    if pending > 1 { "s" } else { "" }
                ));
            }
            for account in output.accounts.iter().filter(|a| a.failure.is_some()) {
                parts.push(format!(
                    "{} : {}.",
                    account.label,
                    account.failure.as_deref().unwrap_or_default()
                ));
            }
            for bank in &output.expired {
                parts.push(format!("Consentement expiré chez {bank} : à renouveler."));
            }
            if output.unlinked > 0 {
                parts.push(format!(
                    "{} compte{} de la banque à rattacher.",
                    output.unlinked,
                    if output.unlinked > 1 { "s" } else { "" }
                ));
            }
            if output.succeeded() {
                reply.redirect(PAGE, &parts.join(" "))
            } else {
                reply.refusal(&parts.join(" "))
            }
        }
        Err(e) => reply.error(e),
    }
}

// ── POST /bank/{id}/delete ───────────────────────────────────────────────────

pub async fn post_bank_delete(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .delete_bank_connection
        .handle(delete_bank_connection::Command {
            connection_id: id.into(),
        })
        .await
    {
        Ok(()) => reply.redirect(PAGE, "Connexion retirée."),
        Err(e) => reply.error(e),
    }
}

// ── GET /fragments/banks ─────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "partials/bank_options.html")]
struct BankOptions {
    banks: Vec<String>,
}

#[derive(Deserialize)]
pub struct BanksParams {
    #[serde(default)]
    country: String,
}

/// Les suggestions du champ « Banque » (une `datalist`), rechargées quand le pays change.
pub async fn get_banks_fragment(
    State(app): AppState,
    Query(params): Query<BanksParams>,
) -> Response {
    let output = app
        .list_banks
        .handle(list_banks::Query {
            country: params.country,
        })
        .await;
    render(BankOptions {
        banks: output.banks,
    })
}
