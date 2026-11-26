//! Vues et outils partagés par plusieurs pages et fragments.

use application::read_net_worth;
use application::read_net_worth::Line;
use asset::AssetKind;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, Local, NaiveDate};
use rust_decimal::Decimal;
use serde::Deserialize;
use shared::{Currency, CurrencyMismatch, Money};
use valuation::Valuation;

use crate::{flash, format};

// ── Vues partagées par la page et les fragments ──────────────────────────────

pub(crate) struct LineView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    /// « Ma banque · •••• 2606 » pour un compte bancaire renseigné : le numéro est masqué dans les listes.
    pub(crate) bank: Option<String>,
    pub(crate) value: String,
    pub(crate) negative: bool,
    pub(crate) valued_on: String,
    /// Signalement d'une valeur absente ou ancienne (règle d'UX 8), sinon `None`.
    pub(crate) freshness: Option<&'static str>,
}

/// « jamais valorisé », « à mettre à jour » (valorisation ancienne, voir `Valuation::is_stale`),
/// ou rien si la valeur est récente.
pub(crate) fn freshness(latest: Option<&Valuation>, today: NaiveDate) -> Option<&'static str> {
    match latest {
        None => Some("jamais valorisé"),
        Some(v) if v.is_stale(today) => Some("à mettre à jour"),
        Some(_) => None,
    }
}

/// La date du jour : la seule lecture de l'horloge du serveur. Chaque handler la lit une fois,
/// puis la passe explicitement aux cas d'usage et aux vues (une requête à minuit reste cohérente).
pub(crate) fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// L'instant présent, pour dater les sessions : avec `today()`, la seule lecture de l'horloge du serveur.
pub(crate) fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

pub(crate) fn date_label(date: NaiveDate) -> String {
    date.format("%d/%m/%Y").to_string()
}

/// La banque et le numéro d'un compte, sur une ligne (`masked` : numéro réduit à ses quatre derniers caractères).
pub(crate) fn bank_line(asset: &asset::Asset, masked: bool) -> Option<String> {
    let number = asset.account_number.as_deref().map(|number| {
        if masked {
            asset::Asset::mask_account_number(number)
        } else {
            asset::Asset::format_account_number(number)
        }
    });
    match (asset.bank_name.as_deref(), number) {
        (Some(bank), Some(number)) => Some(format!("{bank} · {number}")),
        (Some(bank), None) => Some(bank.to_owned()),
        (None, Some(number)) => Some(number),
        (None, None) => None,
    }
}

pub(crate) fn line_views(lines: Vec<Line>, today: NaiveDate) -> Vec<LineView> {
    lines
        .into_iter()
        .map(|line| LineView {
            id: line.asset.id.to_string(),
            bank: bank_line(&line.asset, true),
            name: line.asset.name,
            kind: line.asset.kind.label().to_owned(),
            value: line
                .latest
                .as_ref()
                .map_or_else(|| "—".to_owned(), |v| format::money(&v.value)),
            negative: line
                .latest
                .as_ref()
                .is_some_and(|v| v.value.amount.is_sign_negative()),
            freshness: freshness(line.latest.as_ref(), today),
            valued_on: line
                .latest
                .map_or_else(String::new, |v| date_label(v.valued_on)),
        })
        .collect()
}

/// Le total mis en forme, la raison s'il n'est pas calculable, et son signe.
pub(crate) fn total_view(
    total: &Result<Money, CurrencyMismatch>,
) -> (Option<String>, Option<String>, bool) {
    (
        total.as_ref().ok().map(format::money),
        total.as_ref().err().map(|e| e.to_string()),
        total.as_ref().is_ok_and(|t| t.amount.is_sign_negative()),
    )
}

pub(crate) fn net_worth_query(today: NaiveDate) -> read_net_worth::Query {
    read_net_worth::Query {
        at: today,
        currency: Currency::Eur,
    }
}

/// Un montant saisi dans un formulaire : accepte « 1 234,56 » comme « 1234.56 ».
pub(crate) fn parse_amount(text: &str) -> Option<Decimal> {
    text.replace([' ', '\u{a0}'], "")
        .replace(',', ".")
        .parse()
        .ok()
}

const MONTH_NAMES: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// « octobre 2026 ».
pub(crate) fn month_label(date: NaiveDate) -> String {
    format!("{} {}", MONTH_NAMES[date.month0() as usize], date.year())
}

/// « 2026-10 » : un mois dans une adresse.
pub(crate) fn month_slug(date: NaiveDate) -> String {
    date.format("%Y-%m").to_string()
}

/// Le mois d'une adresse (« 2026-10 »), ou `None` si elle n'en est pas un.
pub(crate) fn parse_month(slug: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{slug}-01"), "%Y-%m-%d").ok()
}

/// Le format attendu par `<input type="date">`.
pub(crate) fn iso_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub(crate) fn kind_options() -> Vec<(&'static str, &'static str)> {
    AssetKind::ALL
        .iter()
        .map(|k| (k.code(), k.label()))
        .collect()
}

pub(crate) fn currency_options() -> Vec<&'static str> {
    Currency::ALL.iter().map(|c| c.code()).collect()
}

/// Rendu d'une page qui affiche un éventuel message flash : le cookie est effacé dès qu'il a été lu,
/// pour que le message ne s'affiche qu'une fois.
pub(crate) fn with_flash_consumed(page: Response, had_flash: bool) -> Response {
    if had_flash {
        ([(header::SET_COOKIE, flash::clear())], page).into_response()
    } else {
        page
    }
}

#[derive(Deserialize)]
pub struct ErrorParams {
    pub(crate) error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomme_et_lit_un_mois() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 17).unwrap();
        assert_eq!(month_label(date), "février 2026");
        assert_eq!(month_slug(date), "2026-02");
        assert_eq!(parse_month("2026-02"), NaiveDate::from_ymd_opt(2026, 2, 1));
        assert_eq!(parse_month("2026-13"), None);
        assert_eq!(parse_month("octobre"), None);
    }
}
