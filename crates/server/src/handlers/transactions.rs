//! Les opérations d'un compte bancaire (`/assets/{id}/transactions`) : la liste, et l'import d'un relevé CSV.
//!
//! L'import est un formulaire classique (fichier envoyé en `multipart`), sans htmx : il fonctionne
//! sans JavaScript, et répond par une redirection avec une confirmation (ou le refus, en haut de la page).

use application::{
    TransactionSource, UseCaseError, categorize_transaction, import_transactions, read_transactions,
};
use askama::Template;
use axum::Form;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use rust_decimal::Decimal;
use serde::Deserialize;

use super::AppState;
use super::reply::{Reply, not_found, render, technical_page};
use super::views::{ErrorParams, date_label, parse_amount, today, with_flash_consumed};
use crate::{flash, format};

/// Au-delà, la page ne montre que les plus récentes : un compte actif en accumule des milliers.
const DISPLAYED: usize = 200;

struct PendingRow {
    date: String,
    label: String,
    amount: String,
    negative: bool,
}

struct TransactionRow {
    id: String,
    /// La catégorie actuelle, pour présélectionner la liste.
    category_id: String,
    date: String,
    label: String,
    amount: String,
    negative: bool,
    /// D'où vient l'opération, en mots (voir la règle d'UX 11).
    source: &'static str,
}

#[derive(Template)]
#[template(path = "transactions_page.html")]
struct TransactionsPage {
    section: &'static str,
    id: String,
    name: String,
    rows: Vec<TransactionRow>,
    /// Les opérations que la banque annonce sans les avoir comptabilisées.
    pending: Vec<PendingRow>,
    /// Leur somme, pour savoir ce qui reste réellement à dépenser.
    pending_total: String,
    /// « 3 opérations », ou « 200 dernières opérations sur 1 250 » quand la liste est tronquée.
    summary: String,
    /// Les catégories proposées pour classer une opération : identifiant et nom.
    categories: Vec<(String, String)>,
    error: Option<String>,
    flash: Option<String>,
}

// ── GET /assets/{id}/transactions ────────────────────────────────────────────

pub async fn get_transactions_page(
    State(app): AppState,
    Path(id): Path<String>,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let output = match app
        .read_transactions
        .handle(read_transactions::Query {
            asset_id: id.clone().into(),
        })
        .await
    {
        Ok(output) => output,
        Err(UseCaseError::Refusal(message)) => {
            return not_found("Compte introuvable", &message);
        }
        Err(e) => return technical_page(e),
    };
    let total = output.transactions.len();
    let rows = output
        .transactions
        .iter()
        .take(DISPLAYED)
        .map(|t| TransactionRow {
            id: t.id.to_string(),
            category_id: t
                .category_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            date: date_label(t.booked_on),
            label: t.label.clone(),
            amount: format::money(&t.amount),
            negative: t.amount.amount.is_sign_negative(),
            source: match t.source {
                TransactionSource::Bank => "Lue chez la banque",
                TransactionSource::Csv => "Importée d'un relevé",
            },
        })
        .collect::<Vec<_>>();
    let pending_total = format::money(&shared::Money::new(
        output.pending.iter().map(|p| p.amount.amount).sum(),
        output.asset.currency,
    ));
    let pending = output
        .pending
        .iter()
        .map(|p| PendingRow {
            date: date_label(p.booked_on),
            label: p.label.clone(),
            amount: format::money(&p.amount),
            negative: p.amount.amount.is_sign_negative(),
        })
        .collect::<Vec<_>>();
    let summary = if total > DISPLAYED {
        format!("{DISPLAYED} dernières opérations sur {total}",)
    } else {
        format::count(total, "opération", "opérations")
    };
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(TransactionsPage {
        section: "assets",
        id,
        name: output.asset.name,
        rows,
        pending,
        pending_total,
        summary,
        categories: output
            .categories
            .into_iter()
            .map(|c| (c.id.to_string(), c.name))
            .collect(),
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

// ── POST /assets/{id}/transactions/{transaction_id}/category ─────────────────

#[derive(Deserialize)]
pub struct CategoryChoice {
    /// Vide pour retirer la catégorie.
    category_id: String,
}

pub async fn post_transaction_category(
    State(app): AppState,
    Path((id, transaction_id)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<CategoryChoice>,
) -> Response {
    let page = format!("/assets/{id}/transactions");
    let reply = Reply::new(&headers, &page);
    let command = categorize_transaction::Command {
        asset_id: id.into(),
        transaction_id: transaction_id.clone().into(),
        category_id: Some(form.category_id)
            .filter(|c| !c.is_empty())
            .map(Into::into),
    };
    match app.categorize_transaction.handle(command).await {
        // On revient sur la ligne modifiée, pour ne pas perdre sa place dans une longue liste.
        Ok(output) => reply.redirect(
            &format!("{page}#t-{transaction_id}"),
            &match output.category_name {
                Some(name) => format!("Classée dans « {name} »."),
                None => "Catégorie retirée.".to_owned(),
            },
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /assets/{id}/transactions/import ────────────────────────────────────

pub async fn post_transactions_import(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Response {
    let page = format!("/assets/{id}/transactions");
    let reply = Reply::new(&headers, &page);

    let mut statement: Option<Vec<u8>> = None;
    let mut closing_balance = String::new();
    loop {
        let field = match form.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return reply.refusal("Le fichier n'a pas pu être lu : réessaie de l'envoyer.");
            }
        };
        match field.name() {
            Some("statement") => match field.bytes().await {
                Ok(bytes) => statement = Some(bytes.to_vec()),
                Err(_) => {
                    return reply
                        .refusal("Le fichier n'a pas pu être lu : il est peut-être trop volumineux (2 Mo au plus).");
                }
            },
            Some("closing_balance") => closing_balance = field.text().await.unwrap_or_default(),
            _ => {}
        }
    }
    let Some(statement) = statement.filter(|bytes| !bytes.is_empty()) else {
        return reply.refusal("Choisis le fichier CSV de ton relevé.");
    };
    let closing_balance = match parse_optional_balance(&closing_balance) {
        Ok(balance) => balance,
        Err(message) => return reply.refusal(&message),
    };

    let command = import_transactions::Command {
        asset_id: id.into(),
        statement: decode_statement(&statement),
        closing_balance,
        today: today(),
    };
    match app.import_transactions.handle(command).await {
        Ok(output) => reply.redirect(&page, &import_message(&output)),
        Err(e) => reply.error(e),
    }
}

fn import_message(output: &import_transactions::Output) -> String {
    let mut message = if output.imported == 0 {
        "Aucune nouvelle opération : le compte les connaissait déjà.".to_owned()
    } else {
        format!(
            "{} importée{}.",
            format::count(output.imported, "opération", "opérations"),
            if output.imported > 1 { "s" } else { "" }
        )
    };
    if output.imported > 0 && output.already_known > 0 {
        message.push_str(&format!(
            " {} déjà connue(s) ignorée(s).",
            output.already_known
        ));
    }
    if let Some(balance) = &output.balance {
        message.push_str(&format!(
            " Solde de {} enregistré.",
            format::money(&balance.value)
        ));
    }
    message
}

/// Le solde est facultatif : vide, on n'enregistre pas de valorisation.
fn parse_optional_balance(text: &str) -> Result<Option<Decimal>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_amount(text).map(Some).ok_or_else(|| {
        format!("Le solde « {text} » n'est pas un nombre : saisis par exemple 1 234,56.")
    })
}

/// Les banques françaises exportent souvent en Windows-1252 plutôt qu'en UTF-8 : sans cette lecture,
/// « Libellé » deviendrait illisible. Un fichier qui n'est pas de l'UTF-8 valide est lu en Windows-1252
/// (identique à Latin-1, sauf le signe euro).
fn decode_statement(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes
            .iter()
            .map(|&b| if b == 0x80 { '€' } else { char::from(b) })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_l_utf8_tel_quel() {
        assert_eq!(
            decode_statement("Libellé;Montant €".as_bytes()),
            "Libellé;Montant €"
        );
    }

    #[test]
    fn lit_le_windows_1252_des_banques() {
        // « Libellé ; 12 € » en Windows-1252 : é = 0xE9, € = 0x80.
        let bytes = [
            b'L', b'i', b'b', b'e', b'l', b'l', 0xE9, b';', b'1', b'2', 0x80,
        ];
        assert_eq!(decode_statement(&bytes), "Libellé;12€");
    }

    #[test]
    fn le_solde_est_facultatif_mais_doit_etre_un_nombre() {
        assert_eq!(parse_optional_balance("  "), Ok(None));
        assert_eq!(
            parse_optional_balance("1 234,56"),
            Ok(Some(Decimal::new(123_456, 2)))
        );
        assert!(parse_optional_balance("beaucoup").is_err());
    }
}
