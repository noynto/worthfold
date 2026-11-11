//! Le tableau de bord (`/`), son total et son graphique (fragments, et SVG autonome sans JavaScript).

use application::{UseCaseError, UseCases, read_net_worth_history};
use askama::Template;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use shared::Currency;

use super::AppState;
use super::reply::{render, technical_fragment, technical_page};
use super::views::{LineView, date_label, line_views, net_worth_query, today, total_view};
use crate::{chart, format};

// ── GET / : tableau de bord ──────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardPage {
    section: &'static str,
    today: String,
    /// Le total mis en forme, ou `None` s'il n'est pas calculable (`total_issue` dit pourquoi).
    total: Option<String>,
    total_issue: Option<String>,
    total_negative: bool,
    asset_count: usize,
    /// Avoirs et dettes mis en forme, « — » s'ils ne sont pas calculables.
    assets_total: String,
    liabilities_total: String,
    has_liabilities: bool,
    /// Les actifs à mettre à jour ou jamais valorisés (règle d'UX 8).
    stale: Vec<LineView>,
}

pub async fn get_dashboard(State(app): AppState) -> Response {
    let today = today();
    let output = match app.read_net_worth.handle(net_worth_query(today)).await {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let (total, total_issue, total_negative) = total_view(&output.total);
    let lines = line_views(output.lines, today);
    let asset_count = lines.len();
    let stale = lines
        .into_iter()
        .filter(|line| line.freshness.is_some())
        .collect();
    let breakdown = output.breakdown.ok();
    render(DashboardPage {
        section: "dashboard",
        today: date_label(today),
        total,
        total_issue,
        total_negative,
        asset_count,
        assets_total: breakdown.map_or_else(|| "—".to_owned(), |b| format::money(&b.assets)),
        liabilities_total: breakdown
            .map_or_else(|| "—".to_owned(), |b| format::money(&b.liabilities)),
        has_liabilities: breakdown.is_some_and(|b| !b.liabilities.amount.is_zero()),
        stale,
    })
}

// ── Fragments htmx : GET /fragments/… ────────────────────────────────────────

#[derive(Template)]
#[template(path = "partials/net_worth.html")]
struct NetWorthFragment {
    today: String,
    total: Option<String>,
    total_issue: Option<String>,
    total_negative: bool,
}

pub async fn get_net_worth_fragment(State(app): AppState) -> Response {
    let today = today();
    match app.read_net_worth.handle(net_worth_query(today)).await {
        Ok(output) => {
            let (total, total_issue, total_negative) = total_view(&output.total);
            render(NetWorthFragment {
                today: date_label(today),
                total,
                total_issue,
                total_negative,
            })
        }
        Err(e) => technical_fragment(e),
    }
}

#[derive(Template)]
#[template(path = "partials/chart.html")]
struct ChartFragment {
    svg: String,
}

pub async fn get_chart_fragment(State(app): AppState) -> Response {
    render(ChartFragment {
        svg: chart_svg(&app).await,
    })
}

/// Le graphique, ou un message à sa place : un complément ne casse jamais la page.
async fn chart_svg(app: &UseCases) -> String {
    let currency = Currency::Eur;
    let query = read_net_worth_history::Query {
        until: today(),
        currency,
    };
    match app.read_net_worth_history.handle(query).await {
        Ok(output) => chart::render(&output.points, currency),
        Err(UseCaseError::Refusal(message)) => chart::message(&message),
        Err(UseCaseError::Technical(e)) => {
            tracing::error!(error = ?e, "Historique du patrimoine illisible.");
            chart::message("L'historique est indisponible pour le moment.")
        }
    }
}

// ── GET /net-worth/chart.svg (page sans JavaScript) ──────────────────────────

pub async fn get_net_worth_chart(State(app): AppState) -> Response {
    let svg = chart::standalone(&chart_svg(&app).await);
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        svg,
    )
        .into_response()
}
