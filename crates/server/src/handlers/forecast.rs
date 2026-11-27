//! La page Prévisions (`/forecast`) : le solde des comptes courants et le patrimoine net mois après mois,
//! en trois scénarios (prudent, attendu, favorable). Une page en lecture seule.

use application::{Forecast, ForecastMonth, HORIZON_MONTHS, read_forecast};
use askama::Template;
use axum::extract::State;
use axum::response::Response;
use chrono::Datelike;
use rust_decimal::Decimal;
use shared::{Currency, Money};

use super::AppState;
use super::reply::{render, technical_page};
use super::views::{month_label, today};
use crate::{chart, format};

/// Le prévisionnel est tenu en euros (seule devise proposée, voir `docs/specs/net-worth.md`).
const CURRENCY: Currency = Currency::Eur;

/// Les horizons mis en avant : dans 3, 6 et 12 mois.
const HORIZONS: [usize; 3] = [3, 6, 12];

struct HorizonView {
    title: String,
    expected: String,
    range: String,
    net_worth: Option<String>,
    expected_negative: bool,
}

struct MonthView {
    label: String,
    movements: String,
    expected: String,
    range: String,
    below_cushion: bool,
    negative: bool,
}

#[derive(Template)]
#[template(path = "forecast_page.html")]
struct ForecastPage {
    section: &'static str,
    start_balance: String,
    horizons: Vec<HorizonView>,
    months: Vec<MonthView>,
    /// Les avertissements, en phrases : découvert, matelas.
    warnings: Vec<String>,
    /// Ce sur quoi le calcul s'appuie, en phrases.
    basis: Vec<String>,
    /// La courbe de la projection, en SVG produit par `chart.rs` (seul `|safe` avec le graphique du tableau de bord).
    chart_svg: String,
}

pub async fn get_forecast_page(State(app): AppState) -> Response {
    let output = match app
        .read_forecast
        .handle(read_forecast::Query {
            today: today(),
            currency: CURRENCY,
        })
        .await
    {
        Ok(output) => output,
        Err(e) => return technical_page(e),
    };
    let money = |amount: Decimal| format::money(&Money::new(amount, CURRENCY));
    let forecast = &output.forecast;
    let range = |m: &ForecastMonth| {
        format!(
            "entre {} et {}",
            money(m.balance.low),
            money(m.balance.high)
        )
    };

    let horizons = HORIZONS
        .iter()
        .filter(|n| **n <= HORIZON_MONTHS)
        .filter_map(|n| forecast.months.get(*n))
        .map(|m| HorizonView {
            title: format!("Fin {}", month_label(m.month)),
            expected: money(m.balance.expected),
            range: range(m),
            // Identique au solde (que des comptes courants) : le répéter n'apprend rien.
            net_worth: m
                .net_worth
                .filter(|w| {
                    (w.expected, w.low, w.high)
                        != (m.balance.expected, m.balance.low, m.balance.high)
                })
                .map(|w| {
                format!(
                    "Patrimoine net : {} ({} – {})",
                    money(w.expected),
                    money(w.low),
                    money(w.high)
                )
            }),
            expected_negative: m.balance.expected < Decimal::ZERO,
        })
        .collect();

    let months = forecast
        .months
        .iter()
        .map(|m| {
            let movements = m.planned_income - m.planned_expenses - m.variable_expenses;
            MonthView {
                label: month_label(m.month),
                movements: format!(
                    "{}{}",
                    if movements.is_sign_negative() {
                        ""
                    } else {
                        "+"
                    },
                    money(movements)
                ),
                expected: money(m.balance.expected),
                range: range(m),
                below_cushion: output.cushion.is_some_and(|c| {
                    m.balance.expected < c.amount && m.balance.expected >= Decimal::ZERO
                }),
                negative: m.balance.expected < Decimal::ZERO,
            }
        })
        .collect();

    let page = ForecastPage {
        section: "forecast",
        start_balance: format::money(&forecast.start_balance),
        horizons,
        months,
        warnings: warnings(forecast, output.cushion, &money),
        basis: basis(&output),
        chart_svg: chart_svg(forecast, today()),
    };
    render(page)
}

/// Les mois à surveiller, dits en toutes lettres : le scénario prudent à découvert, puis le scénario
/// attendu sous le matelas.
fn warnings(
    forecast: &Forecast,
    cushion: Option<Money>,
    money: &dyn Fn(Decimal) -> String,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(m) = forecast.first_below(Decimal::ZERO, |s| s.expected) {
        warnings.push(format!(
            "Dans le scénario attendu, les comptes courants passent à découvert en {}.",
            month_label(m.month)
        ));
    } else if let Some(m) = forecast.first_below(Decimal::ZERO, |s| s.low) {
        warnings.push(format!(
            "Dans le scénario prudent, les comptes courants peuvent passer à découvert dès {} (au pire {}).",
            month_label(m.month),
            money(m.balance.low)
        ));
    }
    if let Some(cushion) = cushion
        && let Some(m) = forecast.first_below(cushion.amount, |s| s.expected)
        && forecast
            .first_below(Decimal::ZERO, |s| s.expected)
            .is_none_or(|d| d.month > m.month)
    {
        warnings.push(format!(
            "Le solde attendu passe sous le matelas de {} en {} : le versement vers le livret serait à reporter.",
            format::money(&cushion),
            month_label(m.month)
        ));
    }
    warnings
}

fn basis(output: &read_forecast::Output) -> Vec<String> {
    let mut basis = Vec::new();
    basis.push(match output.accounts {
        0 => "Aucun compte courant : ajoute un compte bancaire pour partir d'un solde.".to_owned(),
        1 => "Le point de départ est le solde de ton compte courant, d'après sa dernière valorisation.".to_owned(),
        n => format!("Le point de départ est la somme des soldes de tes {n} comptes courants, d'après leur dernière valorisation."),
    });
    if output.pending.0 > 0 {
        basis.push(format!(
            "{} {} en attente chez ta banque ({}) {} déjà retranchée{} du solde de départ.",
            output.pending.0,
            if output.pending.0 == 1 {
                "opération"
            } else {
                "opérations"
            },
            format::money(&Money::new(output.pending.1, CURRENCY)),
            if output.pending.0 == 1 { "est" } else { "sont" },
            if output.pending.0 == 1 { "" } else { "s" },
        ));
    }
    basis.push(if output.payments == 0 {
        "Aucun paiement prévu n'est déclaré : déclare tes revenus et tes charges sur la page Paiements pour un prévisionnel utile.".to_owned()
    } else {
        format!(
            "{} {} : leurs prochaines échéances, avec leur marge, sont comptées aux dates prévues.",
            output.payments,
            if output.payments == 1 { "paiement prévu" } else { "paiements prévus" }
        )
    });
    basis.push(match output.forecast.variable {
        Some(v) => format!(
            "Les autres dépenses (hors paiements déclarés et virements internes) sont évaluées sur les {} dernier(s) mois complet(s) : {} par mois en moyenne, de {} à {}.",
            v.months,
            format::money(&Money::new(v.average, CURRENCY)),
            format::money(&Money::new(v.lowest, CURRENCY)),
            format::money(&Money::new(v.highest, CURRENCY)),
        ),
        None => "Les autres dépenses ne sont pas prévues : il faut au moins un mois complet de relevés importés.".to_owned(),
    });
    basis.push(
        "Prudent : chaque échéance au pire de sa marge et le mois de dépenses le plus cher. Favorable : l'inverse. Les versements vers le livret ne sont pas prévus, et le patrimoine net suppose que les autres actifs ne bougent pas.".to_owned(),
    );
    basis
}

/// Le point de départ (aujourd'hui, un seul solde pour les trois scénarios), puis la fin de chaque mois.
fn chart_svg(forecast: &Forecast, today: chrono::NaiveDate) -> String {
    let start = forecast.start_balance.amount;
    let mut points = vec![chart::ForecastPoint {
        date: today,
        low: start,
        expected: start,
        high: start,
    }];
    points.extend(forecast.months.iter().map(|m| chart::ForecastPoint {
        date: end_of_month(m.month),
        low: m.balance.low,
        expected: m.balance.expected,
        high: m.balance.high,
    }));
    chart::render_forecast(&points, CURRENCY)
}

fn end_of_month(month: chrono::NaiveDate) -> chrono::NaiveDate {
    month
        .checked_add_months(chrono::Months::new(1))
        .and_then(|next| next.with_day(1))
        .and_then(|first| first.pred_opt())
        .unwrap_or(month)
}
