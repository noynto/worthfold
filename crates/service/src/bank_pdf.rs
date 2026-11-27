//! Lecture d'un relevé PDF : une fonction pure, des mots positionnés en entrée, des lignes en sortie.
//!
//! Un PDF n'a pas de colonnes. On les retrouve avec la **position** du texte : la ligne d'en-tête porte
//! « Débit » et « Crédit », et un montant est un débit ou un crédit selon le côté où il se trouve. Les dates
//! n'ont que le jour et le mois : l'année vient de la période du relevé (« du 15 octobre 2026 au 15 novembre 2026 »).
//!
//! Un PDF se lit moins sûrement qu'un CSV : le relevé n'est donc rendu que si le **solde de départ**, les
//! opérations lues et le **solde final** s'accordent. Sinon, une ligne a été mal lue ou manquée, et rien n'est importé.

use chrono::NaiveDate;
use institution::{DecimalSeparator, PdfFormat, PositionedWord};
use rust_decimal::Decimal;

use crate::bank_csv::{StatementLine, normalize, parse_amount};

/// Ce que dit un relevé PDF : ses opérations et ses deux soldes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfStatement {
    pub lines: Vec<StatementLine>,
    pub opening_balance: Decimal,
    pub closing_balance: Decimal,
}

/// Deux mots à moins de cette distance verticale sont sur la même ligne.
const SAME_LINE: f64 = 1.5;
/// Les montants commencent un peu à gauche de l'en-tête de la colonne des débits (montants alignés à droite).
const AMOUNT_ZONE_MARGIN: f64 = 15.0;

struct Line {
    y: f64,
    words: Vec<PositionedWord>,
}

/// Les lignes d'une page, de haut en bas ; les mots de gauche à droite.
fn lines_of(page: &[PositionedWord]) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for word in page {
        match lines.iter_mut().find(|l| (l.y - word.y).abs() < SAME_LINE) {
            Some(line) => line.words.push(word.clone()),
            None => lines.push(Line {
                y: word.y,
                words: vec![word.clone()],
            }),
        }
    }
    lines.sort_by(|a, b| b.y.total_cmp(&a.y));
    for line in &mut lines {
        line.words.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    }
    lines
}

/// Sans espaces, accents ni majuscules : « D ébit » (écrit en deux morceaux dans le PDF) devient « debit ».
fn squash(text: &str) -> String {
    normalize(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Où se trouve ce texte dans la ligne : de l'extrémité gauche du premier mot à la droite du dernier.
fn find_phrase(words: &[PositionedWord], phrase: &str) -> Option<(f64, f64)> {
    let wanted = squash(phrase);
    if wanted.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut owner: Vec<usize> = Vec::new();
    for (i, word) in words.iter().enumerate() {
        for c in squash(&word.text).chars() {
            text.push(c);
            owner.push(i);
        }
    }
    let start = text.find(&wanted)?;
    let first = owner[text[..start].chars().count()];
    let last = owner[text[..start].chars().count() + wanted.chars().count() - 1];
    Some((words[first].x0, words[last].x1))
}

/// Le jour et le mois d'un mot `15.10`.
fn day_month(word: &str, separator: char) -> Option<(u32, u32)> {
    let (day, month) = word.split_once(separator)?;
    let numeric = |t: &str| t.len() == 2 && t.chars().all(|c| c.is_ascii_digit());
    if !numeric(day) || !numeric(month) {
        return None;
    }
    let (day, month) = (day.parse().ok()?, month.parse().ok()?);
    ((1..=31).contains(&day) && (1..=12).contains(&month)).then_some((day, month))
}

const MONTHS: [&str; 12] = [
    "janvier",
    "fevrier",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "aout",
    "septembre",
    "octobre",
    "novembre",
    "decembre",
];

/// Une date à partir du mot `i` : « 15 octobre 2026 » ou « 15/10/2026 ». Rend aussi le nombre de mots lus.
fn date_at(words: &[String], i: usize) -> Option<(NaiveDate, usize)> {
    let word = words.get(i)?;
    if let Some(date) = ["/", ".", "-"]
        .iter()
        .find_map(|sep| NaiveDate::parse_from_str(word, &format!("%d{sep}%m{sep}%Y")).ok())
    {
        return Some((date, 1));
    }
    let day: u32 = words.get(i)?.parse().ok()?;
    let name = words.get(i + 1)?;
    let month = MONTHS.iter().position(|m| m == name)? as u32 + 1;
    let year: i32 = words.get(i + 2)?.parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day).map(|d| (d, 3))
}

/// La période du relevé : « du … au … ».
fn period(pages: &[Vec<PositionedWord>]) -> Option<(NaiveDate, NaiveDate)> {
    let words: Vec<String> = pages
        .iter()
        .flat_map(|page| lines_of(page))
        .flat_map(|line| line.words)
        .map(|w| normalize(&w.text))
        .collect();
    (0..words.len())
        .filter(|&i| words[i] == "du")
        .find_map(|i| {
            let (start, used) = date_at(&words, i + 1)?;
            if words.get(i + 1 + used)? != "au" {
                return None;
            }
            let (end, _) = date_at(&words, i + 2 + used)?;
            Some((start, end))
        })
}

/// L'année d'un jour et mois sans année : la plus récente qui ne dépasse pas la fin de la période
/// (un relevé du 15/12 au 15/01 contient des `28.12` de l'année du début et des `03.01` de celle de la fin).
fn dated(day: u32, month: u32, period: (NaiveDate, NaiveDate)) -> Option<NaiveDate> {
    use chrono::Datelike;
    (period.0.year()..=period.1.year())
        .rev()
        .filter_map(|year| NaiveDate::from_ymd_opt(year, month, day))
        .find(|date| *date <= period.1)
}

struct Operation {
    day: u32,
    month: u32,
    label: Vec<String>,
    amount: Decimal,
}

pub fn parse(pages: &[Vec<PositionedWord>], format: &PdfFormat) -> Result<PdfStatement, String> {
    let separator = format.date_separator.character();
    let end_marker = squash(&format.end_marker);
    let balance_marker = squash(&format.balance_marker);

    let mut operations: Vec<Operation> = Vec::new();
    let mut balances: Vec<Decimal> = Vec::new();
    let mut ended = false;
    let mut found_header = false;

    for page in pages {
        let lines = lines_of(page);
        // L'en-tête du tableau : la ligne qui porte les deux colonnes.
        let Some((header, debit, credit)) = lines.iter().enumerate().find_map(|(i, line)| {
            Some((
                i,
                find_phrase(&line.words, &format.debit_header)?,
                find_phrase(&line.words, &format.credit_header)?,
            ))
        }) else {
            continue;
        };
        found_header = true;
        let zone_start = debit.0 - AMOUNT_ZONE_MARGIN;
        let boundary = ((debit.0 + debit.1) / 2.0 + (credit.0 + credit.1) / 2.0) / 2.0;
        let rows = &lines[header + 1..];
        // L'écart normal entre deux lignes : au-delà, ce n'est plus la suite du libellé (pied de page…).
        let pitch = rows
            .windows(2)
            .map(|w| w[0].y - w[1].y)
            .filter(|gap| *gap > 3.0)
            .fold(f64::INFINITY, f64::min);
        let mut last_y = lines[header].y;
        let mut continuing = !operations.is_empty();

        for row in rows {
            let (amount_words, label_words): (Vec<&PositionedWord>, Vec<&PositionedWord>) =
                row.words.iter().partition(|w| w.x0 >= zone_start);
            let label_text = label_words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let squashed = squash(&label_text);
            let amount = || -> Result<Decimal, String> {
                let text: String = amount_words.iter().map(|w| w.text.as_str()).collect();
                let value = parse_amount(&text, DecimalSeparator::Comma).ok_or_else(|| {
                    format!(
                        "Je ne comprends pas le montant « {text} » de la ligne « {label_text} »."
                    )
                })?;
                let left = amount_words
                    .iter()
                    .map(|w| w.x0)
                    .fold(f64::INFINITY, f64::min);
                let right = amount_words.iter().map(|w| w.x1).fold(f64::MIN, f64::max);
                // Un montant dans la colonne des débits est une dépense.
                Ok(if (left + right) / 2.0 < boundary {
                    -value.abs()
                } else {
                    value.abs()
                })
            };

            if !end_marker.is_empty() && squashed.starts_with(&end_marker) {
                ended = true;
                continue;
            }
            if !balance_marker.is_empty() && squashed.starts_with(&balance_marker) {
                if !amount_words.is_empty() {
                    balances.push(amount()?);
                }
                continuing = false;
                continue;
            }
            let starts_with_date = label_words
                .first()
                .and_then(|w| day_month(&w.text, separator));
            if let (false, Some((day, month))) = (ended, starts_with_date) {
                if amount_words.is_empty() {
                    return Err(format!(
                        "Je ne trouve pas le montant de l'opération du {day:02}{separator}{month:02} : « {label_text} »."
                    ));
                }
                // La date de valeur, dans sa propre colonne après le libellé, n'en fait pas partie.
                let mut text: Vec<String> =
                    label_words.iter().skip(1).map(|w| w.text.clone()).collect();
                if text
                    .last()
                    .is_some_and(|w| day_month(w, separator).is_some())
                {
                    text.pop();
                }
                operations.push(Operation {
                    day,
                    month,
                    label: text,
                    amount: amount()?,
                });
                last_y = row.y;
                continuing = true;
            } else if !ended
                && continuing
                && amount_words.is_empty()
                && !label_words.is_empty()
                && last_y - row.y <= pitch * 1.5
            {
                // La suite du libellé de l'opération précédente.
                if let Some(operation) = operations.last_mut() {
                    operation
                        .label
                        .extend(label_words.iter().map(|w| w.text.clone()));
                }
                last_y = row.y;
            } else {
                continuing = false;
            }
        }
    }

    if !found_header {
        return Err(format!(
            "Je ne retrouve pas les colonnes « {} » et « {} » dans le PDF : vérifie les réglages de la banque.",
            format.debit_header, format.credit_header
        ));
    }
    if operations.is_empty() {
        return Err("Le relevé ne contient aucune opération.".into());
    }
    let period = period(pages).ok_or_else(|| {
        "Je ne retrouve pas la période du relevé (« du 15 octobre 2026 au 15 novembre 2026 ») : \
         elle donne l'année des opérations."
            .to_owned()
    })?;
    let (Some(opening), Some(closing)) = (balances.first().copied(), balances.last().copied())
    else {
        return Err(format!(
            "Je ne retrouve pas les soldes du relevé (lignes qui commencent par « {} »).",
            format.balance_marker
        ));
    };
    if balances.len() < 2 {
        return Err(format!(
            "Je ne trouve qu'un solde sur le relevé (lignes qui commencent par « {} ») : il en faut deux, le solde de départ et le solde final.",
            format.balance_marker
        ));
    }

    let lines = operations
        .into_iter()
        .map(|op| {
            let date = dated(op.day, op.month, period).ok_or_else(|| {
                format!(
                    "La date {:02}/{:02} n'existe pas dans la période du relevé.",
                    op.day, op.month
                )
            })?;
            Ok(StatementLine {
                date,
                label: op.label.join(" "),
                amount: op.amount,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Le contrôle qui rend la lecture fiable : départ + opérations = final.
    let sum: Decimal = lines.iter().map(|l| l.amount).sum();
    if opening + sum != closing {
        return Err(format!(
            "Le relevé ne tombe pas juste : solde de départ {}, {} opérations lues pour {}, soit {} au lieu du solde final {}. \
             Une opération a été mal lue ou manquée : rien n'est importé.",
            euros(opening),
            lines.len(),
            euros(sum),
            euros(opening + sum),
            euros(closing)
        ));
    }
    Ok(PdfStatement {
        lines,
        opening_balance: opening,
        closing_balance: closing,
    })
}

fn euros(amount: Decimal) -> String {
    format!("{} €", amount.round_dp(2).to_string().replace('.', ","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x0: f64, y: f64) -> PositionedWord {
        PositionedWord {
            text: text.into(),
            x0,
            x1: x0 + 5.0 * text.chars().count() as f64,
            y,
        }
    }

    /// Un relevé de deux pages, comme celui d'une banque en ligne : en-têtes en morceaux (« D ébit »), montants alignés
    /// à droite dans leur colonne, libellé sur plusieurs lignes, pied de page, totaux et soldes.
    fn pages() -> Vec<Vec<PositionedWord>> {
        let header = |y: f64| {
            vec![
                word("D", 426.0, y),
                word("ébit", 433.0, y),
                word("C", 516.0, y),
                word("rédit", 524.0, y),
            ]
        };
        // Montants alignés à droite : débit finit à x = 465, crédit à x = 545.
        let debit = |text: &str, y: f64| word(text, 465.0 - 5.0 * text.len() as f64, y);
        let credit = |text: &str, y: f64| word(text, 545.0 - 5.0 * text.len() as f64, y);
        let mut page1 = header(478.0);
        page1.extend([
            word("SOLDE", 74.0, 457.0),
            word("CREDITEUR", 108.0, 457.0),
            word("AU", 163.0, 457.0),
            word("15.10.2026", 178.0, 457.0),
            credit("375,09", 457.0),
            // Une opération antérieure au début de la période.
            word("14.10", 31.0, 442.0),
            word("FACTURE(S)", 74.0, 442.0),
            word("CARTE", 131.0, 442.0),
            word("16.10", 320.0, 442.0),
            debit("9,82", 442.0),
            word("DU", 74.0, 427.0),
            word("141026", 89.0, 427.0),
            word("AUCHAN", 122.0, 427.0),
            // Un pied de page, loin du tableau, avec un montant dans la zone des colonnes.
            word("Hello", 31.0, 100.0),
            word("capital", 400.0, 100.0),
            word("2 203", 480.0, 100.0),
        ]);
        let mut page2 = header(694.0);
        page2.extend([
            word("27.10", 31.0, 659.0),
            word("VIR", 74.0, 659.0),
            word("SEPA", 91.0, 659.0),
            word("27.10", 320.0, 659.0),
            credit("1 025,00", 659.0),
            word("TOTAL", 74.0, 606.0),
            word("DES", 107.0, 606.0),
            word("OPERATIONS", 128.0, 606.0),
            debit("9,82", 606.0),
            credit("1 025,00", 606.0),
            word("SOLDE", 74.0, 589.0),
            word("CREDITEUR", 108.0, 589.0),
            word("AU", 163.0, 589.0),
            word("15.11.2026", 178.0, 589.0),
            credit("1 390,27", 589.0),
            word("Montant", 40.0, 566.0),
            word("de", 79.0, 566.0),
        ]);
        let mut cover = vec![
            word("du", 30.0, 700.0),
            word("15", 45.0, 700.0),
            word("octobre", 60.0, 700.0),
            word("2026", 100.0, 700.0),
            word("au", 130.0, 700.0),
            word("15", 150.0, 700.0),
            word("novembre", 165.0, 700.0),
            word("2026", 210.0, 700.0),
        ];
        cover.extend(page1);
        vec![cover, page2]
    }

    #[test]
    fn lit_un_releve_par_la_position_des_colonnes_et_verifie_les_soldes() {
        let statement = parse(&pages(), &PdfFormat::suggested()).unwrap();
        assert_eq!(statement.opening_balance, Decimal::new(37_509, 2));
        assert_eq!(statement.closing_balance, Decimal::new(139_027, 2));
        assert_eq!(statement.lines.len(), 2);
        // Le débit est une dépense, l'année vient de la période, la date de valeur n'est pas dans le libellé.
        assert_eq!(
            statement.lines[0],
            StatementLine {
                date: NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(),
                label: "FACTURE(S) CARTE DU 141026 AUCHAN".into(),
                amount: Decimal::new(-982, 2),
            }
        );
        // Le crédit, avec un séparateur de milliers en deux mots.
        assert_eq!(statement.lines[1].amount, Decimal::new(102_500, 2));
        assert_eq!(statement.lines[1].label, "VIR SEPA");
    }

    #[test]
    fn refuse_un_releve_dont_les_soldes_ne_tombent_pas_juste() {
        let mut pages = pages();
        // Un solde final faux : par exemple une opération manquée par la lecture.
        for word in pages[1].iter_mut() {
            if word.text == "1 390,27" {
                word.text = "1 400,00".into();
            }
        }
        let error = parse(&pages, &PdfFormat::suggested()).unwrap_err();
        assert!(error.contains("ne tombe pas juste"), "{error}");
        assert!(error.contains("rien n'est importé"), "{error}");
    }

    #[test]
    fn dit_ce_qui_manque_dans_le_pdf() {
        let format = PdfFormat {
            debit_header: "Retrait".into(),
            ..PdfFormat::suggested()
        };
        assert!(
            parse(&pages(), &format)
                .unwrap_err()
                .contains("« Retrait »")
        );
        let without_period: Vec<Vec<PositionedWord>> = pages()
            .into_iter()
            .map(|p| p.into_iter().filter(|w| w.y != 700.0).collect())
            .collect();
        assert!(
            parse(&without_period, &PdfFormat::suggested())
                .unwrap_err()
                .contains("période")
        );
    }

    #[test]
    fn l_annee_d_un_releve_a_cheval_sur_deux_ans() {
        let period = (
            NaiveDate::from_ymd_opt(2026, 12, 15).unwrap(),
            NaiveDate::from_ymd_opt(2027, 1, 15).unwrap(),
        );
        assert_eq!(dated(28, 12, period), NaiveDate::from_ymd_opt(2026, 12, 28));
        assert_eq!(dated(3, 1, period), NaiveDate::from_ymd_opt(2027, 1, 3));
    }
}
