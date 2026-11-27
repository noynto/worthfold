//! La page Paiements (`/payments`) : les paiements prévus, leurs prochaines échéances et ce que l'historique suggère.
//!
//! Les formulaires sont envoyés sans htmx (ils marchent sans JavaScript) : un succès redirige vers la page avec
//! une confirmation (message flash), un refus la rouvre avec le message en haut. La suppression d'un paiement
//! demande confirmation avec htmx (`hx-confirm`).

use application::{
    CalendarLine, OccurrenceStatus, UseCaseError, accept_recurrence_suggestion, add_amount_step,
    add_planned_payment, delete_planned_payment, read_planned_payments, set_occurrence_exception,
};
use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::{Datelike, NaiveDate};
use recurrence::{Amount, AmountStep, DayOfMonth, Direction, Revision, Schedule, Window};
use rust_decimal::Decimal;
use serde::Deserialize;
use shared::{Currency, Money};

use super::AppState;
use super::reply::{Reply, render, technical_page};
use super::views::{
    ErrorParams, date_label, iso_date, month_label, month_slug, parse_amount, parse_month, today,
    with_flash_consumed,
};
use crate::{flash, format};

const PAGE: &str = "/payments";
const CURRENCY: Currency = Currency::Eur;
/// On regarde les échéances des 45 derniers jours (honorées ou manquantes) et des 120 prochains.
const BACK_DAYS: u64 = 45;
const AHEAD_DAYS: u64 = 120;

const MONTHS: [&str; 12] = [
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

// ── Mise en texte ────────────────────────────────────────────────────────────

fn day_text(day: DayOfMonth) -> String {
    match day {
        DayOfMonth::Day(1) => "le 1er".to_owned(),
        DayOfMonth::Day(d) => format!("le {d}"),
        DayOfMonth::Last => "le dernier jour du mois".to_owned(),
    }
}

fn schedule_text(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Monthly { day } => format!("Chaque mois, {}", day_text(*day)),
        Schedule::EveryNMonths {
            n,
            day,
            first_month,
        } => format!(
            "Tous les {n} mois, {}, à partir de {}",
            day_text(*day),
            month_label(*first_month)
        ),
        Schedule::OnMonths { months, day } => format!(
            "Chaque année en {}, {}",
            months
                .iter()
                .map(|m| MONTHS[*m as usize - 1])
                .collect::<Vec<_>>()
                .join(", "),
            day_text(*day)
        ),
        Schedule::Once { date } => format!("Une fois, le {}", date_label(*date)),
    }
}

fn window_text(window: &Window) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(from) = window.from {
        parts.push(format!("à partir du {}", date_label(from)));
    }
    if let Some(until) = window.until {
        parts.push(format!("jusqu'au {}", date_label(until)));
    }
    if let Some(n) = window.occurrences {
        parts.push(format::count(
            n as usize,
            "échéance au total",
            "échéances au total",
        ));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn step_text(step: &AmountStep) -> String {
    if step.margin.amount.is_zero() {
        format::money(&step.value)
    } else {
        format!(
            "{} ± {}",
            format::money(&step.value),
            format::money(&step.margin)
        )
    }
}

fn amount_text(amount: &Amount) -> String {
    let latest = amount.steps.last().map(step_text).unwrap_or_default();
    let mut text = if amount.observed {
        format!("Observé d'après tes relevés (au départ {latest})")
    } else {
        latest
    };
    if let Some(revision) = amount.revision {
        text.push_str(&format!(
            ", révision estimée de {} % chaque {} {}",
            revision.change.normalize().to_string().replace('.', ","),
            revision.day,
            MONTHS[revision.month as usize - 1]
        ));
    }
    text
}

/// Un montant d'échéance, signé, avec sa marge quand c'est une estimation.
fn occurrence_text(line: &CalendarLine) -> String {
    let o = &line.line.occurrence;
    if o.estimated && !o.margin.amount.is_zero() {
        format!(
            "≈ {} ± {}",
            format::money(&o.amount),
            format::money(&o.margin)
        )
    } else if o.estimated {
        format!("≈ {}", format::money(&o.amount))
    } else {
        format::money(&o.amount)
    }
}

// ── La page ──────────────────────────────────────────────────────────────────

struct CalendarRow {
    payment_id: String,
    /// La date au format ISO, pour l'adresse du formulaire.
    iso_date: String,
    date: String,
    label: String,
    amount: String,
    negative: bool,
    /// « Prévu », « Reçu le 05/11/2026 », « Manquant », « Sautée ».
    status: String,
    /// `expected`, `honored`, `missing` ou `skipped` : choisit la couleur.
    state: &'static str,
    overridden: bool,
    /// Une échéance honorée n'a plus rien à corriger.
    editable: bool,
    /// Pour rétablir une échéance sautée ou changée.
    has_exception: bool,
}

struct PaymentRow {
    id: String,
    label: String,
    direction: &'static str,
    account: String,
    schedule: String,
    amount: String,
    window: Option<String>,
    delete_confirm: String,
}

struct SuggestionRow {
    label: String,
    pattern: String,
    direction: &'static str,
    direction_code: &'static str,
    account_id: String,
    account: String,
    schedule: String,
    amount: String,
    detail: String,
}

#[derive(Template)]
#[template(path = "payments_page.html")]
struct PaymentsPage {
    section: &'static str,
    calendar: Vec<CalendarRow>,
    payments: Vec<PaymentRow>,
    suggestions: Vec<SuggestionRow>,
    today_iso: String,
    error: Option<String>,
    flash: Option<String>,
}

#[derive(Template)]
#[template(path = "payment_new_page.html")]
struct PaymentNewPage {
    section: &'static str,
    /// Les comptes bancaires : identifiant et nom.
    accounts: Vec<(String, String)>,
    /// Les catégories : identifiant et nom.
    categories: Vec<(String, String)>,
    months: Vec<(u32, &'static str)>,
    first_months: Vec<(String, String)>,
    days: Vec<u32>,
    error: Option<String>,
}

/// GET /payments/new : la page d'ajout d'un paiement.
pub async fn get_payment_new_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
) -> Response {
    let today = today();
    // Seuls les comptes et les catégories servent ici : la lecture des paiements les fournit déjà.
    let output = match app
        .read_planned_payments
        .handle(read_planned_payments::Query {
            today,
            back_days: BACK_DAYS,
            ahead_days: AHEAD_DAYS,
        })
        .await
    {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    render(PaymentNewPage {
        section: "payments",
        accounts: output
            .accounts
            .iter()
            .map(|a| (a.id.to_string(), a.name.clone()))
            .collect(),
        categories: output
            .categories
            .iter()
            .map(|c| (c.id.to_string(), c.name.clone()))
            .collect(),
        months: MONTHS
            .iter()
            .enumerate()
            .map(|(i, name)| (i as u32 + 1, *name))
            .collect(),
        first_months: first_month_options(today),
        days: (1..=31).collect(),
        error: params.error,
    })
}

pub async fn get_payments_page(
    State(app): AppState,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let today = today();
    let output = match app
        .read_planned_payments
        .handle(read_planned_payments::Query {
            today,
            back_days: BACK_DAYS,
            ahead_days: AHEAD_DAYS,
        })
        .await
    {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let account_name = |id: &shared::AssetId| {
        output
            .accounts
            .iter()
            .find(|a| &a.id == id)
            .map_or_else(|| "compte supprimé".to_owned(), |a| a.name.clone())
    };
    let payment_label = |id: &shared::PlannedPaymentId| {
        output
            .payments
            .iter()
            .find(|p| &p.id == id)
            .map_or_else(String::new, |p| p.label.clone())
    };
    let exceptions_dates = output.exceptions.clone();

    let calendar = output
        .calendar
        .iter()
        .map(|line| {
            let o = &line.line.occurrence;
            let (status, state) = match &line.line.status {
                OccurrenceStatus::Honored { amount, .. } => {
                    (format!("Constaté : {}", format::money(amount)), "honored")
                }
                OccurrenceStatus::Expected => ("Prévu".to_owned(), "expected"),
                OccurrenceStatus::Missing => ("Manquant : rien n'est arrivé".to_owned(), "missing"),
                OccurrenceStatus::Skipped => ("Sautée".to_owned(), "skipped"),
            };
            CalendarRow {
                payment_id: line.payment_id.to_string(),
                iso_date: iso_date(o.date),
                date: date_label(o.date),
                label: payment_label(&line.payment_id),
                amount: occurrence_text(line),
                negative: o.amount.amount.is_sign_negative(),
                status,
                state,
                overridden: o.overridden,
                editable: state != "honored",
                has_exception: exceptions_dates
                    .iter()
                    .any(|(id, date)| id == &line.payment_id && *date == o.date),
            }
        })
        .collect();

    let payments = output
        .payments
        .iter()
        .map(|p| PaymentRow {
            id: p.id.to_string(),
            label: p.label.clone(),
            direction: p.direction.label(),
            account: account_name(&p.account_id),
            schedule: schedule_text(&p.schedule),
            amount: amount_text(&p.amount),
            window: window_text(&p.window),
            delete_confirm: format!(
                "Supprimer le paiement « {} » ? Ses échéances disparaissent du calendrier ; les opérations déjà importées restent.",
                p.label
            ),
        })
        .collect();

    let suggestions = output
        .suggestions
        .iter()
        .map(|s| SuggestionRow {
            label: s.label.clone(),
            pattern: s.pattern.clone(),
            direction: s.direction.label(),
            direction_code: s.direction.code(),
            account_id: s.account_id.to_string(),
            account: account_name(&s.account_id),
            schedule: schedule_text(&s.schedule),
            amount: amount_text(&s.amount),
            detail: format!(
                "{}, la dernière le {}",
                format::count(s.samples, "opération", "opérations"),
                date_label(s.last_date)
            ),
        })
        .collect();

    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(PaymentsPage {
        section: "payments",
        calendar,
        payments,
        suggestions,
        today_iso: iso_date(today),
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

/// Du douzième mois passé au vingt-quatrième mois à venir : un rythme peut avoir commencé l'an dernier.
/// Une liste plutôt qu'un `<input type="month">`, que Firefox et Safari de bureau n'affichent pas comme un
/// sélecteur de mois (on y tape alors du texte que le serveur ne comprend pas).
fn first_month_options(today: NaiveDate) -> Vec<(String, String)> {
    let this_month = today.with_day(1).unwrap_or(today);
    (-12..=24)
        .filter_map(|offset: i32| {
            let months = chrono::Months::new(offset.unsigned_abs());
            if offset < 0 {
                this_month.checked_sub_months(months)
            } else {
                this_month.checked_add_months(months)
            }
        })
        .map(|month| (month_slug(month), month_label(month)))
        .collect()
}

// ── POST /payments ───────────────────────────────────────────────────────────

/// Le formulaire d'un paiement : tout arrive en texte, les refus de lecture sont dits en français.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PaymentForm {
    label: String,
    pattern: String,
    direction: String,
    account_id: String,
    category_id: String,
    // Quand
    schedule: String,
    day: String,
    every_n: String,
    first_month: String,
    month_from: String,
    month_to: String,
    once_date: String,
    // Combien
    amount: String,
    margin: String,
    observed: String,
    amount_from: String,
    revision_month: String,
    revision_day: String,
    revision_change: String,
    // Pendant combien de temps
    window_from: String,
    window_until: String,
    window_count: String,
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

fn parse_day(text: &str) -> Result<DayOfMonth, String> {
    if text == "last" {
        return Ok(DayOfMonth::Last);
    }
    text.trim()
        .parse()
        .map(DayOfMonth::Day)
        .map_err(|_| "Choisis le jour du mois.".to_owned())
}

fn parse_date(text: &str, what: &str) -> Result<Option<NaiveDate>, String> {
    if blank(text) {
        return Ok(None);
    }
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
        .map(Some)
        .map_err(|_| format!("La date {what} n'est pas valide."))
}

fn parse_number<T: std::str::FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{what} : saisis un nombre entier."))
}

/// Les mois d'une plage de l'année, même quand elle passe le 31 décembre (de novembre à février).
fn month_range(from: u32, to: u32) -> Vec<u32> {
    if from <= to {
        (from..=to).collect()
    } else {
        (from..=12).chain(1..=to).collect()
    }
}

fn parse_schedule(form: &PaymentForm) -> Result<Schedule, String> {
    match form.schedule.as_str() {
        "monthly" => Ok(Schedule::Monthly {
            day: parse_day(&form.day)?,
        }),
        "every_n" => Ok(Schedule::EveryNMonths {
            n: parse_number(&form.every_n, "Le rythme en mois")?,
            day: parse_day(&form.day)?,
            first_month: parse_month(form.first_month.trim())
                .ok_or("Choisis le premier mois du rythme.")?,
        }),
        "on_months" => Ok(Schedule::OnMonths {
            months: month_range(
                parse_number(&form.month_from, "Le premier mois")?,
                parse_number(&form.month_to, "Le dernier mois")?,
            ),
            day: parse_day(&form.day)?,
        }),
        "once" => Ok(Schedule::Once {
            date: parse_date(&form.once_date, "de l'échéance")?
                .ok_or("Indique la date de l'échéance.")?,
        }),
        _ => Err("Choisis quand le paiement revient.".to_owned()),
    }
}

fn parse_money(text: &str, what: &str) -> Result<Decimal, String> {
    parse_amount(text).ok_or_else(|| {
        format!("{what} « {text} » n'est pas un nombre : saisis par exemple 1 234,56.")
    })
}

fn parse_form(
    form: &PaymentForm,
    today: NaiveDate,
) -> Result<add_planned_payment::Command, String> {
    let direction = Direction::from_code(&form.direction)
        .ok_or("Choisis s'il s'agit d'une entrée ou d'une sortie d'argent.")?;
    let window = Window {
        from: parse_date(&form.window_from, "de début")?,
        until: parse_date(&form.window_until, "de fin")?,
        occurrences: if blank(&form.window_count) {
            None
        } else {
            Some(parse_number(&form.window_count, "Le nombre d'échéances")?)
        },
    };
    let value = parse_money(&form.amount, "Le montant")?;
    let margin = if blank(&form.margin) {
        Decimal::ZERO
    } else {
        parse_money(&form.margin, "La marge")?
    };
    let revision = if blank(&form.revision_month) {
        None
    } else {
        Some(Revision {
            month: parse_number(&form.revision_month, "Le mois de révision")?,
            day: if blank(&form.revision_day) {
                1
            } else {
                parse_number(&form.revision_day, "Le jour de révision")?
            },
            change: if blank(&form.revision_change) {
                Decimal::ZERO
            } else {
                parse_money(&form.revision_change, "La variation")?
            },
        })
    };
    // Le premier palier vaut dès le début : la date de départ du paiement, sinon aujourd'hui
    // (les échéances plus anciennes que le premier palier prennent ce premier montant).
    let from = parse_date(&form.amount_from, "du montant")?
        .or(window.from)
        .unwrap_or(today);
    Ok(add_planned_payment::Command {
        label: form.label.clone(),
        pattern: form.pattern.clone(),
        direction,
        account_id: form.account_id.clone().into(),
        category_id: (!blank(&form.category_id)).then(|| form.category_id.clone().into()),
        schedule: parse_schedule(form)?,
        amount: Amount {
            steps: vec![AmountStep {
                from,
                value: Money::new(value, CURRENCY),
                margin: Money::new(margin, CURRENCY),
            }],
            revision,
            observed: !blank(&form.observed),
        },
        window,
    })
}

pub async fn post_payment(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<PaymentForm>,
) -> Response {
    // Un refus rouvre la page d'ajout ; un succès revient à la liste.
    let reply = Reply::new(&headers, "/payments/new");
    let command = match parse_form(&form, today()) {
        Ok(command) => command,
        Err(message) => return reply.refusal(&message),
    };
    match app.add_planned_payment.handle(command).await {
        Ok(output) => reply.redirect(PAGE, &format!("« {} » ajouté.", output.payment.label)),
        Err(e) => reply.error(e),
    }
}

// ── POST /payments/{id}/delete ───────────────────────────────────────────────

pub async fn post_payment_delete(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    match app
        .delete_planned_payment
        .handle(delete_planned_payment::Command {
            payment_id: id.into(),
        })
        .await
    {
        Ok(output) => reply.redirect(PAGE, &format!("« {} » supprimé.", output.payment.label)),
        Err(e) => reply.error(e),
    }
}

// ── POST /payments/{id}/steps ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StepForm {
    from: String,
    amount: String,
    margin: String,
}

pub async fn post_payment_step(
    State(app): AppState,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<StepForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let parsed = (|| {
        let from = parse_date(&form.from, "du nouveau montant")?
            .ok_or("Indique à partir de quand le nouveau montant s'applique.")?;
        let value = parse_money(&form.amount, "Le montant")?;
        let margin = if blank(&form.margin) {
            Decimal::ZERO
        } else {
            parse_money(&form.margin, "La marge")?
        };
        Ok::<_, String>((from, value, margin))
    })();
    let (from, value, margin) = match parsed {
        Ok(parsed) => parsed,
        Err(message) => return reply.refusal(&message),
    };
    match app
        .add_amount_step
        .handle(add_amount_step::Command {
            payment_id: id.into(),
            from,
            value,
            margin,
        })
        .await
    {
        Ok(output) => reply.redirect(
            PAGE,
            &format!(
                "Nouveau montant de « {} » enregistré à partir du {}.",
                output.payment.label,
                date_label(from)
            ),
        ),
        Err(e) => reply.error(e),
    }
}

// ── POST /payments/{id}/occurrences/{date} ───────────────────────────────────

#[derive(Deserialize)]
pub struct OccurrenceForm {
    /// `skip`, `amount` ou `clear`.
    action: String,
    #[serde(default)]
    amount: String,
}

pub async fn post_occurrence(
    State(app): AppState,
    Path((id, date)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<OccurrenceForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let Ok(date) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
        return reply.refusal("La date de l'échéance n'est pas valide.");
    };
    let request = match form.action.as_str() {
        "skip" => set_occurrence_exception::Request::Skip,
        "clear" => set_occurrence_exception::Request::Clear,
        "amount" => match parse_money(&form.amount, "Le montant") {
            Ok(amount) => set_occurrence_exception::Request::Amount(amount),
            Err(message) => return reply.refusal(&message),
        },
        _ => return reply.refusal("Cette action n'existe pas."),
    };
    let message = match form.action.as_str() {
        "skip" => "sautée",
        "clear" => "rétablie",
        _ => "modifiée",
    };
    match app
        .set_occurrence_exception
        .handle(set_occurrence_exception::Command {
            payment_id: id.into(),
            date,
            request,
        })
        .await
    {
        Ok(output) => reply.redirect(
            PAGE,
            &format!(
                "Échéance du {} de « {} » {message}.",
                date_label(date),
                output.payment_label
            ),
        ),
        Err(UseCaseError::Refusal(text)) => reply.refusal(&text),
        Err(e) => reply.error(e),
    }
}

// ── POST /payments/suggestions ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SuggestionForm {
    account_id: String,
    pattern: String,
    direction: String,
}

pub async fn post_suggestion(
    State(app): AppState,
    headers: HeaderMap,
    Form(form): Form<SuggestionForm>,
) -> Response {
    let reply = Reply::new(&headers, PAGE);
    let Some(direction) = Direction::from_code(&form.direction) else {
        return reply.refusal("Le sens de la suggestion est invalide.");
    };
    match app
        .accept_recurrence_suggestion
        .handle(accept_recurrence_suggestion::Command {
            account_id: form.account_id.into(),
            pattern: form.pattern,
            direction,
        })
        .await
    {
        Ok(output) => reply.redirect(PAGE, &format!("« {} » ajouté.", output.payment.label)),
        Err(e) => reply.error(e),
    }
}

#[cfg(test)]
mod first_month_tests {
    use super::*;

    #[test]
    fn les_mois_proposes_vont_de_l_an_dernier_a_dans_deux_ans_et_se_relisent() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let options = first_month_options(today);
        assert_eq!(options.len(), 37);
        assert_eq!(options.first().unwrap().0, "2025-11");
        assert_eq!(options.last().unwrap().0, "2028-11");
        assert!(options.contains(&("2026-12".to_owned(), "décembre 2026".to_owned())));
        // Chaque identifiant est celui que `parse_month` sait relire.
        assert!(options.iter().all(|(slug, _)| parse_month(slug).is_some()));
    }
}
