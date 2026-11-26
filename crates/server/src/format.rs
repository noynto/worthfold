//! Mise en forme des valeurs pour l'affichage, à la française. Pure présentation :
//! le domaine ne connaît ni les séparateurs ni les symboles.

use rust_decimal::{Decimal, RoundingStrategy};
use shared::{Currency, Money};

/// Espace fine insécable (U+202F) : séparateur de milliers en typographie française.
const THOUSANDS: char = '\u{202F}';
/// Espace insécable (U+00A0) : entre le montant et son symbole, pour ne jamais les couper.
const BEFORE_SYMBOL: char = '\u{00A0}';

/// `12345.6 EUR` → `12 345,60 €` ; `-1500 EUR` → `−1 500,00 €`.
pub fn money(value: &Money) -> String {
    format!(
        "{}{BEFORE_SYMBOL}{}",
        amount(value.amount),
        symbol(value.currency)
    )
}

fn amount(value: Decimal) -> String {
    let rounded = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let digits = format!("{:.2}", rounded.abs());
    let (integer, decimals) = digits.split_once('.').unwrap_or((&digits, "00"));

    let mut grouped = String::new();
    for (i, digit) in integer.chars().enumerate() {
        if i > 0 && (integer.len() - i) % 3 == 0 {
            grouped.push(THOUSANDS);
        }
        grouped.push(digit);
    }

    // Vrai signe moins (U+2212), plus lisible que le tiret.
    let sign = if rounded.is_sign_negative() && !rounded.is_zero() {
        "−"
    } else {
        ""
    };
    format!("{sign}{grouped},{decimals}")
}

/// Version courte pour les axes d'un graphique : `245 k€`, `1,2 M€`, `−80 k€`.
pub fn money_short(value: f64, currency: Currency) -> String {
    let (scaled, unit, decimals) = match value.abs() {
        v if v >= 1_000_000.0 => (value / 1_000_000.0, "M", 2),
        v if v >= 10_000.0 => (value / 1_000.0, "k", 0),
        v if v >= 1_000.0 => (value / 1_000.0, "k", 1),
        _ => (value, "", 0),
    };
    let mut number = format!("{:.*}", decimals, scaled.abs()).replace('.', ",");
    // Les zéros de fin ne disent rien (« 1,30 » → « 1,3 », « 2,00 » → « 2 »).
    if number.contains(',') {
        number.truncate(number.trim_end_matches('0').trim_end_matches(',').len());
    }
    let sign = if scaled < 0.0 && number != "0" {
        "−"
    } else {
        ""
    };
    let separator = if unit.is_empty() {
        BEFORE_SYMBOL.to_string()
    } else {
        THOUSANDS.to_string()
    };
    format!("{sign}{number}{separator}{unit}{}", symbol(currency))
}

/// Un nombre accordé : `count(0, …)` → « aucune valorisation », `count(1, "valorisation", …)` → « 1 valorisation »,
/// `count(3, …, "valorisations")` → « 3 valorisations ». Avec un déterminant (« sa valorisation »), 1 n'est pas répété.
pub fn count(n: usize, singular: &str, plural: &str) -> String {
    match n {
        0 => format!("aucune {}", singular.rsplit(' ').next().unwrap_or(singular)),
        1 if singular.contains(' ') => singular.to_owned(),
        1 => format!("1 {singular}"),
        _ if plural.contains(' ') => {
            let (determiner, noun) = plural.split_once(' ').unwrap_or(("", plural));
            format!("{determiner} {n} {noun}")
        }
        _ => format!("{n} {plural}"),
    }
}

pub fn symbol(currency: Currency) -> &'static str {
    match currency {
        Currency::Eur => "€",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eur(s: &str) -> String {
        money(&Money::new(s.parse().unwrap(), Currency::Eur))
    }

    #[test]
    fn groupe_les_milliers_et_utilise_la_virgule() {
        assert_eq!(eur("1234567.891"), "1\u{202F}234\u{202F}567,89\u{a0}€");
    }

    #[test]
    fn affiche_toujours_deux_decimales() {
        assert_eq!(eur("100"), "100,00\u{a0}€");
        assert_eq!(eur("0"), "0,00\u{a0}€");
        assert_eq!(eur("999.5"), "999,50\u{a0}€");
    }

    #[test]
    fn arrondit_au_centime_le_plus_proche() {
        assert_eq!(eur("0.005"), "0,01\u{a0}€");
        assert_eq!(eur("999.999"), "1\u{202F}000,00\u{a0}€");
    }

    #[test]
    fn affiche_un_vrai_signe_moins() {
        assert_eq!(eur("-1500"), "−1\u{202F}500,00\u{a0}€");
        assert_eq!(eur("-0.001"), "0,00\u{a0}€");
    }

    #[test]
    fn abrege_les_grands_montants_pour_les_axes() {
        assert_eq!(money_short(245_000.0, Currency::Eur), "245\u{202F}k€");
        assert_eq!(money_short(1_300_000.0, Currency::Eur), "1,3\u{202F}M€");
        assert_eq!(money_short(2_500.0, Currency::Eur), "2,5\u{202F}k€");
        assert_eq!(money_short(-80_000.0, Currency::Eur), "−80\u{202F}k€");
        assert_eq!(money_short(0.0, Currency::Eur), "0\u{a0}€");
        // Des graduations voisines au-delà du million restent distinctes.
        assert_eq!(money_short(2_450_000.0, Currency::Eur), "2,45\u{202F}M€");
        assert_eq!(money_short(2_500_000.0, Currency::Eur), "2,5\u{202F}M€");
    }

    #[test]
    fn accorde_les_nombres() {
        assert_eq!(
            count(0, "valorisation", "valorisations"),
            "aucune valorisation"
        );
        assert_eq!(count(1, "valorisation", "valorisations"), "1 valorisation");
        assert_eq!(count(3, "valorisation", "valorisations"), "3 valorisations");
        assert_eq!(
            count(1, "sa valorisation", "ses valorisations"),
            "sa valorisation"
        );
        assert_eq!(
            count(3, "sa valorisation", "ses valorisations"),
            "ses 3 valorisations"
        );
    }
}
