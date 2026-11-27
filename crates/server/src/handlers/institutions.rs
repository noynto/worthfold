//! La page Banques (`/banks`) : chaque banque dit comment décoder ses exports CSV.
//!
//! Les formulaires sont envoyés sans htmx (ils marchent sans JavaScript) : un succès redirige vers la page
//! avec une confirmation (message flash), un refus la rouvre avec le message en haut. La suppression demande
//! confirmation avec htmx (`hx-confirm`), comme ailleurs.

use application::{
    AmountColumns, CsvFormat, DateFormat, DaySeparator, DecimalSeparator, Delimiter, Encoding,
    FormatInput, PdfFormat, add_institution, delete_institution, read_institutions,
    update_institution,
};
use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;

use super::AppState;
use super::reply::{Reply, render, technical_page};
use super::views::{ErrorParams, with_flash_consumed};
use crate::{flash, format};

const PAGE: &str = "/banks";

/// Les réglages d'une banque, tels que les champs du formulaire les montrent.
struct FormatView {
    encoding: &'static str,
    delimiter: &'static str,
    header_line: u32,
    date_column: String,
    date_format: &'static str,
    label_column: String,
    /// Deux colonnes (débit et crédit) plutôt qu'une colonne de montant signé.
    split: bool,
    amount_column: String,
    debit_column: String,
    credit_column: String,
    decimal_separator: &'static str,
    invert_sign: bool,
    /// Le relevé PDF : la banque en fournit-elle, et comment le lire.
    pdf_enabled: bool,
    pdf_date_separator: &'static str,
    pdf_debit_header: String,
    pdf_credit_header: String,
    pdf_balance_marker: String,
    pdf_end_marker: String,
}

impl FormatView {
    fn new(csv: &CsvFormat, pdf: Option<&PdfFormat>) -> Self {
        let enabled = pdf.is_some();
        let pdf = pdf.cloned().unwrap_or_else(PdfFormat::suggested);
        Self {
            pdf_enabled: enabled,
            pdf_date_separator: pdf.date_separator.code(),
            pdf_debit_header: pdf.debit_header,
            pdf_credit_header: pdf.credit_header,
            pdf_balance_marker: pdf.balance_marker,
            pdf_end_marker: pdf.end_marker,
            ..Self::from(csv)
        }
    }
}

impl From<&CsvFormat> for FormatView {
    fn from(csv: &CsvFormat) -> Self {
        let (split, amount, debit, credit) = match &csv.amount {
            AmountColumns::Signed(column) => (false, column.clone(), String::new(), String::new()),
            AmountColumns::DebitCredit { debit, credit } => {
                (true, String::new(), debit.clone(), credit.clone())
            }
        };
        Self {
            encoding: csv.encoding.code(),
            delimiter: csv.delimiter.code(),
            header_line: csv.header_line,
            date_column: csv.date_column.clone(),
            date_format: csv.date_format.code(),
            label_column: csv.label_column.clone(),
            split,
            amount_column: amount,
            debit_column: debit,
            credit_column: credit,
            decimal_separator: csv.decimal_separator.code(),
            invert_sign: csv.invert_sign,
            pdf_enabled: false,
            pdf_date_separator: "",
            pdf_debit_header: String::new(),
            pdf_credit_header: String::new(),
            pdf_balance_marker: String::new(),
            pdf_end_marker: String::new(),
        }
    }
}

struct BankView {
    name: String,
    /// « 2 comptes », « aucun compte ».
    accounts: String,
    /// Le format en une phrase, pour la liste repliée.
    summary: String,
    form: FormatView,
    delete_confirm: String,
}

type Options = Vec<(&'static str, &'static str)>;

#[derive(Template)]
#[template(path = "banks_page.html")]
struct BanksPage {
    section: &'static str,
    banks: Vec<BankView>,
    /// Les réglages proposés pour une nouvelle banque.
    new_form: FormatView,
    encodings: Options,
    delimiters: Options,
    date_formats: Options,
    decimal_separators: Options,
    day_separators: Options,
    error: Option<String>,
    flash: Option<String>,
}

fn summary(csv: &CsvFormat, pdf: Option<&PdfFormat>) -> String {
    let amount = match &csv.amount {
        AmountColumns::Signed(column) => format!("montant « {column} »"),
        AmountColumns::DebitCredit { debit, credit } => {
            format!("débit « {debit} », crédit « {credit} »")
        }
    };
    let pdf = if pdf.is_some() {
        " · relevé PDF géré"
    } else {
        ""
    };
    format!(
        "{}, séparateur {}, en-tête ligne {} : date « {} » ({}), libellé « {} », {amount}{pdf}",
        csv.encoding.label(),
        csv.delimiter.label(),
        csv.header_line,
        csv.date_column,
        csv.date_format.label(),
        csv.label_column,
    )
}

// ── GET /banks ───────────────────────────────────────────────────────────────

pub async fn get_banks_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let output = match app.read_institutions.handle(read_institutions::Query).await {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let banks = output
        .institutions
        .into_iter()
        .map(|line| BankView {
            accounts: match line.accounts {
                0 => "aucun compte".to_owned(),
                n => format::count(n, "compte", "comptes"),
            },
            summary: summary(&line.institution.csv, line.institution.pdf.as_ref()),
            form: FormatView::new(&line.institution.csv, line.institution.pdf.as_ref()),
            delete_confirm: format!(
                "Supprimer la banque « {} » ? Les opérations déjà importées ne changent pas.",
                line.institution.name
            ),
            name: line.institution.name,
        })
        .collect();
    let options = |all: Vec<(&'static str, &'static str)>| all;
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(BanksPage {
        section: "banks",
        banks,
        new_form: FormatView::new(&CsvFormat::suggested(), None),
        day_separators: DaySeparator::ALL
            .iter()
            .map(|v| (v.code(), v.label()))
            .collect(),
        encodings: options(
            Encoding::ALL
                .iter()
                .map(|v| (v.code(), v.label()))
                .collect(),
        ),
        delimiters: options(
            Delimiter::ALL
                .iter()
                .map(|v| (v.code(), v.label()))
                .collect(),
        ),
        date_formats: options(
            DateFormat::ALL
                .iter()
                .map(|v| (v.code(), v.label()))
                .collect(),
        ),
        decimal_separators: options(
            DecimalSeparator::ALL
                .iter()
                .map(|v| (v.code(), v.label()))
                .collect(),
        ),
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

// ── POST /banks et /banks/{name}/update ──────────────────────────────────────

#[derive(Deserialize)]
pub struct BankForm {
    /// Seulement à la création : le nom d'une banque ne change pas.
    #[serde(default)]
    name: String,
    encoding: String,
    delimiter: String,
    header_line: String,
    date_column: String,
    date_format: String,
    label_column: String,
    amount_mode: String,
    #[serde(default)]
    amount_column: String,
    #[serde(default)]
    debit_column: String,
    #[serde(default)]
    credit_column: String,
    decimal_separator: String,
    /// Une case à cocher : absente quand elle ne l'est pas.
    #[serde(default)]
    invert_sign: Option<String>,
    #[serde(default)]
    pdf_enabled: Option<String>,
    #[serde(default)]
    pdf_date_separator: String,
    #[serde(default)]
    pdf_debit_header: String,
    #[serde(default)]
    pdf_credit_header: String,
    #[serde(default)]
    pdf_balance_marker: String,
    #[serde(default)]
    pdf_end_marker: String,
}

impl BankForm {
    fn format(&self) -> FormatInput {
        FormatInput {
            encoding: self.encoding.clone(),
            delimiter: self.delimiter.clone(),
            header_line: self.header_line.clone(),
            date_column: self.date_column.clone(),
            date_format: self.date_format.clone(),
            label_column: self.label_column.clone(),
            amount_mode: self.amount_mode.clone(),
            amount_column: self.amount_column.clone(),
            debit_column: self.debit_column.clone(),
            credit_column: self.credit_column.clone(),
            decimal_separator: self.decimal_separator.clone(),
            invert_sign: self.invert_sign.is_some(),
            pdf_enabled: self.pdf_enabled.is_some(),
            pdf_date_separator: self.pdf_date_separator.clone(),
            pdf_debit_header: self.pdf_debit_header.clone(),
            pdf_credit_header: self.pdf_credit_header.clone(),
            pdf_balance_marker: self.pdf_balance_marker.clone(),
            pdf_end_marker: self.pdf_end_marker.clone(),
        }
    }
}

pub async fn post_institution(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<BankForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let command = add_institution::Command {
        format: form.format(),
        name: form.name,
    };
    match app.add_institution.handle(command).await {
        Ok(output) => reply.redirect(
            PAGE,
            &format!(
                "« {} » ajoutée : tu peux la choisir pour tes comptes bancaires.",
                output.institution.name
            ),
        ),
        Err(e) => reply.error(e),
    }
}

pub async fn post_institution_update(
    State(app): AppState,
    Path(name): Path<String>,
    headers: HeaderMap,
    Form(form): Form<BankForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let command = update_institution::Command {
        format: form.format(),
        name,
    };
    match app.update_institution.handle(command).await {
        Ok(output) => reply.redirect(
            PAGE,
            &format!("Réglages de « {} » enregistrés.", output.institution.name),
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /banks/{name}/delete ────────────────────────────────────────────────

pub async fn post_institution_delete(
    State(app): AppState,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .delete_institution
        .handle(delete_institution::Command { name })
        .await
    {
        Ok(output) => reply.redirect(PAGE, &format!("« {} » supprimée.", output.institution.name)),
        Err(e) => reply.error(e),
    }
}
