//! La page Budget (`/budget`, `/budget/{mois}`) : la synthèse d'un mois, les enveloppes par catégorie,
//! et le versement d'épargne suggéré.
//!
//! Le mois est dans l'adresse (`/budget/2026-10`) : un refus de formulaire y revient sans le perdre.
//! Les formulaires sont envoyés sans htmx (ils marchent sans JavaScript), comme ceux des catégories.

use application::{
    EnvelopeState, SavingsLimit, UseCaseError, read_budget, save_savings_plan, set_envelope,
};
use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use budget::{month_start, next_month, previous_month};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use shared::{Currency, Money};

use super::AppState;
use super::reply::{Reply, not_found, render, technical_page};
use super::views::{
    ErrorParams, month_label, month_slug, parse_amount, parse_month, today, with_flash_consumed,
};
use crate::{flash, format};

/// Le budget est tenu en euros (seule devise proposée, voir `docs/specs/net-worth.md`).
const CURRENCY: Currency = Currency::Eur;

struct EnvelopeView {
    category_id: String,
    name: String,
    limit: String,
    spent: String,
    /// « Reste 120,00 € » ou « Dépassé de 50,00 € » : l'état dit en toutes lettres.
    status: String,
    /// Part consommée pour la jauge, plafonnée à 100.
    gauge: u32,
    percent: u32,
    /// `within`, `near` ou `over` : choisit la couleur de la jauge.
    state: &'static str,
    remove_confirm: String,
}

struct SummaryView {
    income: String,
    spending: String,
    result: String,
    result_negative: bool,
    /// Le nombre d'opérations sans catégorie du mois, dites en toutes lettres.
    uncategorized: Option<String>,
}

struct SavingsView {
    destination: String,
    current_balance: String,
    destination_balance: String,
    /// Le matelas retenu et d'où il vient, ou pourquoi il manque.
    cushion: String,
    cushion_note: String,
    headline: String,
    detail: String,
    /// Faux pour un mois passé : les soldes sont ceux d'aujourd'hui.
    is_current_month: bool,
}

#[derive(Template)]
#[template(path = "budget_page.html")]
struct BudgetPage {
    section: &'static str,
    month_label: String,
    month_slug: String,
    previous: String,
    /// Absent pour le mois en cours : le futur n'a pas de relevé.
    next: Option<String>,
    summary: SummaryView,
    envelopes: Vec<EnvelopeView>,
    /// Les catégories de dépense : identifiant et nom.
    expense_categories: Vec<(String, String)>,
    /// Les livrets proposés comme destination : identifiant, nom, et s'il est le livret choisi.
    savings_books: Vec<(String, String, bool)>,
    savings: Option<SavingsView>,
    /// Le réglage actuel, pour préremplir le formulaire.
    ceiling_value: String,
    cushion_value: String,
    error: Option<String>,
    flash: Option<String>,
}

/// Un montant saisi à nouveau dans un champ : « 22950 », « 1500,5 ».
fn plain(amount: Decimal) -> String {
    amount.normalize().to_string().replace('.', ",")
}

// ── GET /budget et /budget/{mois} ────────────────────────────────────────────

pub async fn get_budget_current(
    state: AppState,
    params: Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    show(state, month_start(today()), params, headers).await
}

pub async fn get_budget_page(
    state: AppState,
    Path(month): Path<String>,
    params: Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    match parse_month(&month) {
        Some(month) => show(state, month, params, headers).await,
        None => not_found(
            "Mois introuvable",
            "Cette adresse ne désigne pas un mois : elle doit ressembler à /budget/2026-10.",
        ),
    }
}

async fn show(
    State(app): AppState,
    month: NaiveDate,
    Query(params): Query<ErrorParams>,
    headers: HeaderMap,
) -> Response {
    let today = today();
    let output = match app
        .read_budget
        .handle(read_budget::Query {
            month,
            today,
            currency: CURRENCY,
        })
        .await
    {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let current_month = month_start(today);
    let summary = &output.summary;
    let money = |amount: Decimal| format::money(&Money::new(amount, CURRENCY));

    let envelopes = output
        .envelopes
        .iter()
        .map(|status| EnvelopeView {
            category_id: status.category.id.to_string(),
            name: status.category.name.clone(),
            limit: format::money(&status.limit),
            spent: format::money(&status.spent),
            status: if status.state == EnvelopeState::Over {
                format!(
                    "Dépassé de {}",
                    money(-status.remaining.amount)
                )
            } else {
                format!("Reste {}", format::money(&status.remaining))
            },
            gauge: status.percent.min(100),
            percent: status.percent,
            state: match status.state {
                EnvelopeState::Within => "within",
                EnvelopeState::Near => "near",
                EnvelopeState::Over => "over",
            },
            remove_confirm: format!(
                "Retirer l'enveloppe « {} » à partir de {} ? Les mois précédents gardent leur plafond.",
                status.category.name,
                month_label(output.month)
            ),
        })
        .collect();

    let savings = output.savings.as_ref().map(|report| {
        let cushion = report.cushion;
        let (cushion_text, cushion_note) = match (output.plan.as_ref().and_then(|p| p.cushion_override), report.evaluated_cushion) {
            (Some(fixed), _) => (
                format::money(&fixed),
                "fixé par toi".to_owned(),
            ),
            (None, Some(evaluated)) => (
                format::money(&evaluated.amount),
                if evaluated.months == 1 {
                    "évalué : les dépenses du dernier mois complet, arrondies aux 50 € supérieurs".to_owned()
                } else {
                    format!(
                        "évalué : le mois le plus cher des {} derniers mois complets, arrondi aux 50 € supérieurs",
                        evaluated.months
                    )
                },
            ),
            (None, None) => (
                "à évaluer".to_owned(),
                "il faut au moins un mois complet de relevés importés, ou tu le fixes toi-même".to_owned(),
            ),
        };
        let (headline, detail) = savings_message(report, cushion, &report.destination.name);
        SavingsView {
            destination: report.destination.name.clone(),
            current_balance: format::money(&report.current_balance),
            destination_balance: format::money(&report.destination_balance),
            cushion: cushion_text,
            cushion_note,
            headline,
            detail,
            is_current_month: output.month == current_month,
        }
    });

    let chosen = output.plan.as_ref().map(|p| p.destination.clone());
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(BudgetPage {
        section: "budget",
        month_label: month_label(output.month),
        month_slug: month_slug(output.month),
        previous: month_slug(previous_month(output.month)),
        next: (output.month < current_month).then(|| month_slug(next_month(output.month))),
        summary: SummaryView {
            income: money(summary.income),
            spending: money(summary.spending()),
            result: money(summary.result()),
            result_negative: summary.result().is_sign_negative() && !summary.result().is_zero(),
            uncategorized: (summary.uncategorized_count > 0).then(|| {
                format::count(
                    summary.uncategorized_count,
                    "opération sans catégorie",
                    "opérations sans catégorie",
                )
            }),
        },
        envelopes,
        expense_categories: output
            .expense_categories
            .iter()
            .map(|c| (c.id.to_string(), c.name.clone()))
            .collect(),
        savings_books: output
            .savings_books
            .iter()
            .map(|a| {
                (
                    a.id.to_string(),
                    a.name.clone(),
                    Some(&a.id) == chosen.as_ref(),
                )
            })
            .collect(),
        savings,
        ceiling_value: plain(
            output
                .plan
                .as_ref()
                .map_or(budget::SavingsPlan::default_ceiling(CURRENCY), |p| {
                    p.ceiling
                })
                .amount,
        ),
        cushion_value: output
            .plan
            .as_ref()
            .and_then(|p| p.cushion_override)
            .map(|c| plain(c.amount))
            .unwrap_or_default(),
        error: params.error,
        flash,
    });
    with_flash_consumed(page, had_flash)
}

/// Le titre et l'explication du versement suggéré : le montant, ou la raison pour laquelle il n'y en a pas.
fn savings_message(
    report: &read_budget::SavingsReport,
    cushion: Option<Money>,
    destination: &str,
) -> (String, String) {
    let (Some(cushion), Some(s)) = (cushion, report.suggestion) else {
        return (
            "Pas encore de versement à suggérer".to_owned(),
            "Le matelas n'est pas connu : importe au moins un mois complet de relevés, ou fixe-le toi-même ci-dessous."
                .to_owned(),
        );
    };
    let money = |m: Money| format::money(&m);
    if s.amount.amount > Decimal::ZERO {
        let detail = match s.limited_by {
            SavingsLimit::Surplus => {
                let already = if s.already_saved.amount > Decimal::ZERO {
                    format!(", après {} déjà virés", money(s.already_saved))
                } else {
                    String::new()
                };
                format!(
                    "C'est ce qu'il reste de ce mois{already}. Ton compte gardera au moins {}.",
                    money(cushion)
                )
            }
            SavingsLimit::Cushion => format!(
                "Il reste {} sur le mois, mais seulement {} au-dessus du matelas de {} : on s'arrête là.",
                money(s.surplus),
                money(s.above_cushion),
                money(cushion)
            ),
            SavingsLimit::Ceiling => format!(
                "Le livret ne peut plus recevoir que {} avant son plafond.",
                money(s.room)
            ),
        };
        return (
            format!("Verse {} sur {destination}", money(s.amount)),
            detail,
        );
    }
    let detail = if s.surplus.amount.is_zero() {
        "Ce mois-ci, tes dépenses dépassent tes revenus : il n'y a rien à verser.".to_owned()
    } else if s.room.amount.is_zero() {
        "Le livret a atteint son plafond.".to_owned()
    } else if s.above_cushion.amount.is_zero() {
        format!(
            "Ton compte est au niveau du matelas de {} : verser davantage ferait passer dessous.",
            money(cushion)
        )
    } else {
        "Tout ce qui restait de ce mois a déjà été viré.".to_owned()
    };
    ("Rien à verser".to_owned(), detail)
}

// ── POST /budget/{mois}/envelopes ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EnvelopeForm {
    category_id: String,
    limit: String,
}

pub async fn post_envelope(
    State(app): AppState,
    Path(month): Path<String>,
    headers: HeaderMap,
    Form(form): Form<EnvelopeForm>,
) -> Response {
    let page = format!("/budget/{month}");
    let reply = Reply::new(&headers, &page);
    let Some(from_month) = parse_month(&month) else {
        return reply.refusal("Ce mois est invalide.");
    };
    let Some(limit) = parse_amount(&form.limit) else {
        return reply.refusal(&format!(
            "Le plafond « {} » n'est pas un nombre : saisis par exemple 400 ou 450,50.",
            form.limit
        ));
    };
    set(
        &app,
        &reply,
        &page,
        form.category_id,
        Some(limit),
        from_month,
    )
    .await
}

// ── POST /budget/{mois}/envelopes/{category_id}/remove ───────────────────────

pub async fn post_envelope_remove(
    State(app): AppState,
    Path((month, category_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let page = format!("/budget/{month}");
    let reply = Reply::new(&headers, &page);
    let Some(from_month) = parse_month(&month) else {
        return reply.refusal("Ce mois est invalide.");
    };
    set(&app, &reply, &page, category_id, None, from_month).await
}

async fn set(
    app: &application::UseCases,
    reply: &Reply,
    page: &str,
    category_id: String,
    limit: Option<Decimal>,
    from_month: NaiveDate,
) -> Response {
    let command = set_envelope::Command {
        category_id: category_id.into(),
        limit,
        currency: CURRENCY,
        from_month,
    };
    match app.set_envelope.handle(command).await {
        Ok(output) => {
            let when = month_label(output.envelope.from_month);
            reply.redirect(
                page,
                &match output.envelope.limit {
                    Some(limit) => format!(
                        "Plafond de « {} » fixé à {} à partir de {when}.",
                        output.category.name,
                        format::money(&limit)
                    ),
                    None => format!(
                        "Enveloppe « {} » retirée à partir de {when}.",
                        output.category.name
                    ),
                },
            )
        }
        Err(e) => reply.error(e),
    }
}

// ── POST /budget/{mois}/savings ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SavingsForm {
    destination: String,
    ceiling: String,
    /// Vide : le matelas est évalué d'après les dépenses.
    cushion: String,
}

pub async fn post_savings_plan(
    State(app): AppState,
    Path(month): Path<String>,
    headers: HeaderMap,
    Form(form): Form<SavingsForm>,
) -> Response {
    let page = format!("/budget/{month}");
    let reply = Reply::new(&headers, &page);
    let Some(ceiling) = parse_amount(&form.ceiling) else {
        return reply.refusal(&format!(
            "Le plafond « {} » n'est pas un nombre : saisis par exemple 22 950.",
            form.ceiling
        ));
    };
    let cushion = if form.cushion.trim().is_empty() {
        None
    } else {
        match parse_amount(&form.cushion) {
            Some(cushion) => Some(cushion),
            None => {
                return reply.refusal(&format!(
                    "Le matelas « {} » n'est pas un nombre : saisis par exemple 1 500, ou laisse vide pour l'évaluer.",
                    form.cushion
                ));
            }
        }
    };
    let command = save_savings_plan::Command {
        destination: form.destination.into(),
        ceiling,
        cushion,
        currency: CURRENCY,
    };
    match app.save_savings_plan.handle(command).await {
        Ok(_) => reply.redirect(&page, "Réglage de l'épargne enregistré."),
        Err(UseCaseError::Refusal(message)) => reply.refusal(&message),
        Err(e) => reply.error(e),
    }
}
