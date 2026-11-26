//! Graphique d'évolution du patrimoine, dessiné en SVG côté serveur.
//! La page l'intègre comme fragment htmx (`/fragments/chart`) ; ses règles de style (`chart-*`) sont dans `app.css` ;
//! sans JavaScript, `/net-worth/chart.svg` le sert en document autonome (voir [`standalone`]).

use std::fmt::Write;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use shared::{Currency, Money};

use crate::format;

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 260.0;
const LEFT: f64 = 84.0;
const RIGHT: f64 = 16.0;
const TOP: f64 = 16.0;
const BOTTOM: f64 = 36.0;
/// Le graphique de l'historique, ou un message s'il n'y a encore rien à tracer.
pub fn render(points: &[(NaiveDate, Money)], currency: Currency) -> String {
    if points.is_empty() {
        return message("L'évolution s'affichera dès la première valorisation.");
    }

    let values: Vec<f64> = points
        .iter()
        .map(|(_, m)| m.amount.to_f64().unwrap_or(0.0))
        .collect();
    let ticks = nice_ticks(
        values.iter().copied().fold(f64::INFINITY, f64::min),
        values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (y_min, y_max) = (ticks[0], ticks[ticks.len() - 1]);

    let first = points[0].0;
    let span_days = (points[points.len() - 1].0 - first).num_days() as f64;
    let x = |date: NaiveDate| {
        if span_days == 0.0 {
            LEFT + (WIDTH - LEFT - RIGHT) / 2.0
        } else {
            LEFT + (date - first).num_days() as f64 / span_days * (WIDTH - LEFT - RIGHT)
        }
    };
    let y = |value: f64| TOP + (y_max - value) / (y_max - y_min) * (HEIGHT - TOP - BOTTOM);

    let mut svg = open("Évolution du patrimoine");

    grid(&mut svg, &ticks, &y, currency);

    date_labels(
        &mut svg,
        &points.iter().map(|(d, _)| *d).collect::<Vec<_>>(),
        &x,
    );

    // Courbe en escalier : entre deux saisies, le patrimoine vaut la dernière valeur connue
    // (le même choix que le calcul du total). Une pente inventerait une évolution progressive.
    let coords: Vec<(f64, f64)> = points
        .iter()
        .zip(&values)
        .map(|((date, _), value)| (x(*date), y(*value)))
        .collect();
    if coords.len() > 1 {
        let mut steps = vec![coords[0]];
        for pair in coords.windows(2) {
            steps.push((pair[1].0, pair[0].1));
            steps.push(pair[1]);
        }
        let line: String = steps
            .iter()
            .map(|(cx, cy)| format!("{cx:.1},{cy:.1}"))
            .collect::<Vec<_>>()
            .join(" ");
        let baseline = y(y_min.max(0.0).min(y_max));
        let _ = write!(
            svg,
            r#"<path class="chart-area" d="M{:.1},{baseline:.1} L{} L{:.1},{baseline:.1} Z"/><polyline class="chart-curve" points="{line}"/>"#,
            coords[0].0,
            line.replace(' ', " L"),
            coords[coords.len() - 1].0
        );
    }
    for ((cx, cy), (date, money)) in coords.iter().zip(points) {
        let _ = write!(
            svg,
            r#"<circle class="chart-dot" cx="{cx:.1}" cy="{cy:.1}" r="4"><title>{} · {}</title></circle>"#,
            date.format("%d/%m/%Y"),
            format::money(money)
        );
    }

    svg.push_str("</svg>");
    svg
}

/// Grille horizontale et graduations ; la ligne du zéro est en pointillés quand elle est dans l'échelle.
fn grid(svg: &mut String, ticks: &[f64], y: &impl Fn(f64) -> f64, currency: Currency) {
    let y_min = ticks[0];
    for tick in ticks {
        let ty = y(*tick);
        let class = if *tick == 0.0 && ticks.len() > 1 && y_min < 0.0 {
            "chart-zero"
        } else {
            "chart-grid"
        };
        let _ = write!(
            svg,
            r#"<line class="{class}" x1="{LEFT}" x2="{}" y1="{ty:.1}" y2="{ty:.1}"/>"#,
            WIDTH - RIGHT
        );
        let _ = write!(
            svg,
            r#"<text class="chart-label" x="{}" y="{:.1}" text-anchor="end">{}</text>"#,
            LEFT - 8.0,
            ty + 4.0,
            format::money_short(*tick, currency)
        );
    }
}

/// Dates : la première, la dernière et celle du milieu si la place le permet.
fn date_labels(svg: &mut String, dates: &[NaiveDate], x: &impl Fn(NaiveDate) -> f64) {
    let mut labelled = vec![0, dates.len() - 1];
    if dates.len() > 2 {
        labelled.insert(1, dates.len() / 2);
    }
    labelled.dedup();
    for (i, &index) in labelled.iter().enumerate() {
        let date = dates[index];
        let anchor = if labelled.len() == 1 {
            "middle"
        } else if i == 0 {
            "start"
        } else if i == labelled.len() - 1 {
            "end"
        } else {
            "middle"
        };
        let _ = write!(
            svg,
            r#"<text class="chart-label" x="{:.1}" y="{}" text-anchor="{anchor}">{}</text>"#,
            x(date),
            HEIGHT - 10.0,
            date.format("%d/%m/%Y")
        );
    }
}

/// Le graphique de la projection : le solde attendu en trait plein, la fourchette entre le scénario
/// prudent et le favorable en bande. Une pente relie les fins de mois : la projection avance mois par mois.
pub fn render_forecast(points: &[ForecastPoint], currency: Currency) -> String {
    if points.len() < 2 {
        return message("La projection s'affichera dès qu'il y aura un compte courant valorisé.");
    }
    let to_f64 = |m: Decimal| m.to_f64().unwrap_or(0.0);
    let lows: Vec<f64> = points.iter().map(|p| to_f64(p.low)).collect();
    let highs: Vec<f64> = points.iter().map(|p| to_f64(p.high)).collect();
    let expected: Vec<f64> = points.iter().map(|p| to_f64(p.expected)).collect();
    let ticks = nice_ticks(
        lows.iter().copied().fold(f64::INFINITY, f64::min),
        highs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (y_min, y_max) = (ticks[0], ticks[ticks.len() - 1]);
    let first = points[0].date;
    let span_days = (points[points.len() - 1].date - first).num_days().max(1) as f64;
    let x = |date: NaiveDate| {
        LEFT + (date - first).num_days() as f64 / span_days * (WIDTH - LEFT - RIGHT)
    };
    let y = |value: f64| TOP + (y_max - value) / (y_max - y_min) * (HEIGHT - TOP - BOTTOM);

    let mut svg = open("Projection du solde des comptes courants");
    grid(&mut svg, &ticks, &y, currency);
    date_labels(
        &mut svg,
        &points.iter().map(|p| p.date).collect::<Vec<_>>(),
        &x,
    );

    let path = |values: &[f64]| {
        points
            .iter()
            .zip(values)
            .map(|(p, v)| format!("{:.1},{:.1}", x(p.date), y(*v)))
            .collect::<Vec<_>>()
            .join(" L")
    };
    let band: Vec<String> = points
        .iter()
        .zip(&highs)
        .map(|(p, v)| format!("{:.1},{:.1}", x(p.date), y(*v)))
        .chain(
            points
                .iter()
                .zip(&lows)
                .rev()
                .map(|(p, v)| format!("{:.1},{:.1}", x(p.date), y(*v))),
        )
        .collect();
    let _ = write!(
        svg,
        r#"<path class="chart-area" d="M{} Z"/><polyline class="chart-bound" points="{}"/><polyline class="chart-bound" points="{}"/><path class="chart-curve" fill="none" d="M{}"/>"#,
        band.join(" L"),
        path(&highs).replace(" L", " "),
        path(&lows).replace(" L", " "),
        path(&expected)
    );
    for (p, value) in points.iter().zip(&expected) {
        let _ = write!(
            svg,
            r#"<circle class="chart-dot" cx="{:.1}" cy="{:.1}" r="4"><title>{} · {} (entre {} et {})</title></circle>"#,
            x(p.date),
            y(*value),
            p.date.format("%d/%m/%Y"),
            format::money(&Money::new(p.expected, currency)),
            format::money(&Money::new(p.low, currency)),
            format::money(&Money::new(p.high, currency)),
        );
    }
    svg.push_str("</svg>");
    svg
}

/// Un point de la projection : le solde attendu et sa fourchette à une date.
pub struct ForecastPoint {
    pub date: NaiveDate,
    pub low: Decimal,
    pub expected: Decimal,
    pub high: Decimal,
}

/// Un SVG qui n'affiche qu'un message, centré. Le texte vient de l'application : on l'échappe quand même.
pub fn message(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let mut svg = open("Graphique indisponible");
    let _ = write!(
        svg,
        r#"<text class="chart-message" x="{}" y="{}" text-anchor="middle">{escaped}</text></svg>"#,
        WIDTH / 2.0,
        HEIGHT / 2.0
    );
    svg
}

fn open(label: &str) -> String {
    format!(
        r#"<svg class="chart" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}" role="img" aria-label="{label}">"#
    )
}

/// Le même graphique en document SVG autonome, pour `/net-worth/chart.svg` (page sans JavaScript) :
/// il charge alors lui-même la feuille de style de l'application.
pub fn standalone(svg: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><?xml-stylesheet href="/static/app.css" type="text/css"?>{svg}"#
    )
}

/// Des graduations « rondes » (1, 2 ou 5 × 10ⁿ) qui encadrent les valeurs.
fn nice_ticks(min: f64, max: f64) -> Vec<f64> {
    let (min, max) = if min == max {
        let pad = (min.abs() * 0.1).max(1.0);
        (min - pad, max + pad)
    } else {
        (min, max)
    };
    // Environ cinq graduations : le pas se calcule sur l'écart réel, puis s'arrondit.
    let step = nice((max - min) / 4.0);
    let low = (min / step).floor() * step;
    let high = (max / step).ceil() * step;
    let count = ((high - low) / step).round() as usize;
    (0..=count)
        .map(|i| low + i as f64 * step)
        .map(|t| if t.abs() < step * 1e-9 { 0.0 } else { t })
        .collect()
}

fn nice(value: f64) -> f64 {
    let exponent = value.log10().floor();
    let fraction = value / 10f64.powf(exponent);
    let nice_fraction = match fraction {
        f if f < 1.5 => 1.0,
        f if f < 3.0 => 2.0,
        f if f < 7.0 => 5.0,
        _ => 10.0,
    };
    nice_fraction * 10f64.powf(exponent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(day: u32, amount: i64) -> (NaiveDate, Money) {
        (
            NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            Money::new(Decimal::from(amount), Currency::Eur),
        )
    }

    fn forecast_point(month: u32, low: i64, expected: i64, high: i64) -> ForecastPoint {
        ForecastPoint {
            date: NaiveDate::from_ymd_opt(2026, month, 28).unwrap(),
            low: Decimal::from(low),
            expected: Decimal::from(expected),
            high: Decimal::from(high),
        }
    }

    #[test]
    fn la_projection_trace_le_solde_attendu_et_sa_fourchette() {
        let svg = render_forecast(
            &[
                forecast_point(10, 1000, 1000, 1000),
                forecast_point(11, 800, 1200, 1500),
                forecast_point(12, 600, 1400, 2000),
            ],
            Currency::Eur,
        );
        assert_eq!(svg.matches(r#"class="chart-bound""#).count(), 2);
        assert_eq!(svg.matches("<circle").count(), 3);
        assert!(svg.contains("Projection du solde"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn la_projection_sans_deux_points_affiche_un_message() {
        let svg = render_forecast(&[forecast_point(11, 1, 1, 1)], Currency::Eur);
        assert!(svg.contains("chart-message"));
    }

    #[test]
    fn les_graduations_encadrent_les_valeurs_avec_un_pas_rond() {
        assert_eq!(
            nice_ticks(62_000.0, 245_000.0),
            vec![50_000.0, 100_000.0, 150_000.0, 200_000.0, 250_000.0]
        );
        assert_eq!(
            nice_ticks(-180_000.0, 60_000.0),
            vec![
                -200_000.0, -150_000.0, -100_000.0, -50_000.0, 0.0, 50_000.0, 100_000.0
            ]
        );
        assert_eq!(
            nice_ticks(20_000.0, 270_700.0),
            vec![
                0.0, 50_000.0, 100_000.0, 150_000.0, 200_000.0, 250_000.0, 300_000.0
            ]
        );
    }

    #[test]
    fn une_valeur_unique_reste_lisible() {
        let ticks = nice_ticks(100.0, 100.0);
        assert!(ticks[0] < 100.0 && *ticks.last().unwrap() > 100.0);
    }

    #[test]
    fn trace_une_courbe_et_un_point_par_date() {
        let svg = render(
            &[point(1, 1000), point(5, 1500), point(9, 1200)],
            Currency::Eur,
        );
        assert!(svg.contains(r#"class="chart-curve""#));
        assert_eq!(svg.matches(r#"class="chart-dot""#).count(), 3);
        assert!(svg.contains("01/11/2026") && svg.contains("09/11/2026"));
    }

    #[test]
    fn un_seul_point_n_a_pas_de_courbe() {
        let svg = render(&[point(9, 1000)], Currency::Eur);
        assert!(!svg.contains(r#"class="chart-curve""#));
        assert_eq!(svg.matches(r#"class="chart-dot""#).count(), 1);
    }

    #[test]
    fn le_svg_n_embarque_aucun_style() {
        // Les règles `chart-*` sont dans `app.css` : un `<style>` en ligne obligerait la CSP à tolérer le style en ligne.
        let svg = render(&[point(1, 1000), point(9, 1200)], Currency::Eur);
        assert!(!svg.contains("<style") && !svg.contains("style="));
    }

    #[test]
    fn le_graphique_est_un_document_xml_bien_forme() {
        let svg = standalone(&render(&[point(1, 1000), point(9, 1200)], Currency::Eur));
        // Équilibre des balises, sans dépendance : chaque ouvrante non autofermante a sa fermante.
        let mut stack = Vec::new();
        for tag in svg.split('<').skip(1).map(|t| t.split('>').next().unwrap()) {
            if tag.starts_with('?') || tag.ends_with('/') {
                continue;
            }
            let name = tag
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap();
            if tag.starts_with('/') {
                assert_eq!(
                    stack.pop(),
                    Some(name),
                    "balise fermante inattendue : {name}"
                );
            } else {
                stack.push(name);
            }
        }
        assert!(stack.is_empty(), "balises non fermées : {stack:?}");
    }

    #[test]
    fn sans_historique_affiche_un_message() {
        assert!(render(&[], Currency::Eur).contains("première valorisation"));
    }
}
