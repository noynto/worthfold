//! Lecture d'un export CSV de relevé bancaire : une fonction pure, du texte en entrée, des lignes en sortie.
//!
//! Les banques n'ont pas de format commun. On reconnaît donc le fichier à ses **en-têtes** plutôt qu'à
//! l'ordre des colonnes :
//! - la séparation (`;`, `,` ou tabulation) est devinée sur la première ligne utile ;
//! - les lignes avant l'en-tête (nom du compte, période…) sont ignorées ;
//! - une colonne de date, une de libellé, puis soit une colonne de montant (signé),
//!   soit deux colonnes débit et crédit ;
//! - les dates `jj/mm/aaaa`, `aaaa-mm-jj`, `jj-mm-aaaa` ; les montants `1 234,56`, `-1234.56`, `12,50 €`.

use chrono::NaiveDate;
use rust_decimal::Decimal;

/// Une ligne du relevé : ce que le fichier dit, avant de devenir une transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementLine {
    pub date: NaiveDate,
    pub label: String,
    /// Signé : négatif pour un débit.
    pub amount: Decimal,
}

/// Les colonnes utiles, repérées par leur position.
struct Columns {
    date: usize,
    label: usize,
    amount: Amount,
}

enum Amount {
    Signed(usize),
    DebitCredit { debit: usize, credit: usize },
}

/// Lit un relevé. Le refus est un message en français, prêt à être montré tel quel.
pub fn parse(text: &str) -> Result<Vec<StatementLine>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let delimiter = guess_delimiter(text)?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut columns: Option<Columns> = None;
    let mut lines = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("Le fichier n'est pas un CSV lisible : {e}."))?;
        let fields: Vec<&str> = record.iter().collect();
        if fields.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let Some(columns) = &columns else {
            columns = find_columns(&fields);
            continue;
        };
        let line_number = record.position().map_or(0, |p| p.line());
        if let Some(line) = read_line(columns, &fields, line_number)? {
            lines.push(line);
        }
    }
    if columns.is_none() {
        return Err(
            "Je ne retrouve pas l'en-tête du relevé : il faut une colonne de date, une de \
                    libellé et une de montant (ou deux, débit et crédit)."
                .into(),
        );
    }
    if lines.is_empty() {
        return Err("Le relevé ne contient aucune opération.".into());
    }
    Ok(lines)
}

fn guess_delimiter(text: &str) -> Result<u8, String> {
    // La ligne qui compte le plus de séparateurs a toutes les chances d'être l'en-tête.
    let best = text
        .lines()
        .map(|line| {
            b";\t,"
                .iter()
                .copied()
                .map(|d| (line.bytes().filter(|b| *b == d).count(), d))
                .max_by_key(|(count, _)| *count)
                .unwrap_or((0, b';'))
        })
        .max_by_key(|(count, _)| *count);
    match best {
        Some((count, delimiter)) if count > 0 => Ok(delimiter),
        _ => Err("Le fichier est vide ou n'est pas un CSV (aucun séparateur trouvé).".into()),
    }
}

/// Minuscules et sans accents, pour comparer des en-têtes écrits de dix façons.
fn normalize(header: &str) -> String {
    header
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' | 'ä' => 'a',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

fn find_columns(fields: &[&str]) -> Option<Columns> {
    let headers: Vec<String> = fields.iter().map(|f| normalize(f)).collect();
    let position = |predicate: &dyn Fn(&str) -> bool| headers.iter().position(|h| predicate(h));

    // Plusieurs dates (opération, valeur…) : on préfère celle de l'opération.
    let date = position(&|h| h.starts_with("date") && h.contains("operation"))
        .or_else(|| position(&|h| h.starts_with("date") && !h.contains("valeur")))
        .or_else(|| position(&|h| h.starts_with("date")))?;
    let label = position(&|h| {
        [
            "libelle",
            "label",
            "description",
            "intitule",
            "detail",
            "motif",
        ]
        .iter()
        .any(|word| h.contains(word))
    })?;
    let amount = match position(&|h| h.contains("montant") || h.contains("amount")) {
        Some(index) => Amount::Signed(index),
        None => Amount::DebitCredit {
            debit: position(&|h| h.contains("debit"))?,
            credit: position(&|h| h.contains("credit"))?,
        },
    };
    Some(Columns {
        date,
        label,
        amount,
    })
}

fn read_line(
    columns: &Columns,
    fields: &[&str],
    line_number: u64,
) -> Result<Option<StatementLine>, String> {
    let field = |index: usize| fields.get(index).map_or("", |f| f.trim());
    let amount_text: Vec<&str> = match columns.amount {
        Amount::Signed(index) => vec![field(index)],
        Amount::DebitCredit { debit, credit } => vec![field(debit), field(credit)],
    };
    // Un pied de page (« Solde », total…) n'a ni date ni montant : on l'ignore.
    if field(columns.date).is_empty() && amount_text.iter().all(|a| a.is_empty()) {
        return Ok(None);
    }

    let date = parse_date(field(columns.date)).ok_or_else(|| {
        format!(
            "Ligne {line_number} : la date « {} » n'est pas reconnue (attendu : jj/mm/aaaa ou aaaa-mm-jj).",
            field(columns.date)
        )
    })?;
    let amount = match columns.amount {
        Amount::Signed(_) => parse_amount(amount_text[0]),
        Amount::DebitCredit { .. } => {
            let debit = parse_optional_amount(amount_text[0])?.map(|d| -d.abs());
            let credit = parse_optional_amount(amount_text[1])?.map(|c| c.abs());
            match (debit, credit) {
                (Some(d), None) => Ok(d),
                (None, Some(c)) => Ok(c),
                (Some(d), Some(c)) => Ok(d + c),
                (None, None) => Err(()),
            }
        }
    }
    .map_err(|()| {
        format!(
            "Ligne {line_number} : le montant « {} » n'est pas un nombre.",
            amount_text.join(" ").trim()
        )
    })?;
    let label = field(columns.label)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Some(StatementLine {
        date,
        label,
        amount,
    }))
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    ["%d/%m/%Y", "%Y-%m-%d", "%d-%m-%Y", "%d/%m/%y", "%d.%m.%Y"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(text, format).ok())
}

/// `Ok(None)` pour une cellule vide (la colonne débit d'un crédit, par exemple).
fn parse_optional_amount(text: &str) -> Result<Option<Decimal>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    parse_amount(text)
        .map(Some)
        .map_err(|()| format!("le montant « {text} » n'est pas un nombre."))
}

fn parse_amount(text: &str) -> Result<Decimal, ()> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '€' && !c.is_alphabetic())
        .map(|c| if c == '−' { '-' } else { c })
        .collect();
    // « 1.234,56 » : le point sépare les milliers quand une virgule suit.
    let cleaned = if cleaned.contains(',') {
        cleaned.replace('.', "").replace(',', ".")
    } else {
        cleaned
    };
    cleaned.parse::<Decimal>().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn lit_un_releve_a_montant_signe_en_format_francais() {
        let csv = "Date;Libellé;Montant\n\
                   05/11/2026;PRLV LOYER  NOVEMBRE;-700,00\n\
                   06/11/2026;VIR SALAIRE;2 150,50\n";
        let lines = parse(csv).unwrap();
        assert_eq!(
            lines,
            vec![
                StatementLine {
                    date: day(2026, 11, 5),
                    label: "PRLV LOYER NOVEMBRE".into(),
                    amount: Decimal::new(-70000, 2),
                },
                StatementLine {
                    date: day(2026, 11, 6),
                    label: "VIR SALAIRE".into(),
                    amount: Decimal::new(215050, 2),
                },
            ]
        );
    }

    #[test]
    fn lit_les_colonnes_debit_et_credit() {
        let csv = "Date opération;Date valeur;Libellé;Débit;Crédit\n\
                   05/11/2026;06/11/2026;CB CARREFOUR;45,20;\n\
                   07/11/2026;07/11/2026;VIR SALAIRE;;1 800,00\n";
        let lines = parse(csv).unwrap();
        assert_eq!(lines[0].date, day(2026, 11, 5)); // la date d'opération, pas celle de valeur
        assert_eq!(lines[0].amount, Decimal::new(-4520, 2));
        assert_eq!(lines[1].amount, Decimal::new(180000, 2));
    }

    #[test]
    fn un_debit_deja_negatif_reste_un_debit() {
        let csv = "Date;Libellé;Débit;Crédit\n05/11/2026;CB;-45,20;\n";
        assert_eq!(parse(csv).unwrap()[0].amount, Decimal::new(-4520, 2));
    }

    #[test]
    fn devine_la_virgule_et_les_dates_iso() {
        let csv = "date,label,amount\n2026-11-05,Coffee,-3.50\n";
        let lines = parse(csv).unwrap();
        assert_eq!(lines[0].date, day(2026, 11, 5));
        assert_eq!(lines[0].amount, Decimal::new(-350, 2));
    }

    #[test]
    fn ignore_les_lignes_avant_l_en_tete_et_les_pieds_de_page() {
        let csv = "\u{feff}Compte courant;FR76 0000;\n\
                   Période;01/11/2026 - 30/11/2026;\n\
                   \n\
                   Date;Libellé;Montant\n\
                   05/11/2026;Loyer;-700,00\n\
                   ;Solde final;\n";
        let lines = parse(csv).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].label, "Loyer");
    }

    #[test]
    fn comprend_les_milliers_au_point_le_symbole_euro_et_le_moins_typographique() {
        assert_eq!(parse_amount("1.234,56 €"), Ok(Decimal::new(123456, 2)));
        assert_eq!(parse_amount("−12,50"), Ok(Decimal::new(-1250, 2)));
        assert_eq!(parse_amount("1\u{a0}000,00"), Ok(Decimal::new(100000, 2)));
        assert_eq!(parse_amount("abc"), Err(()));
    }

    #[test]
    fn refuse_une_date_illisible_en_citant_la_ligne() {
        let csv = "Date;Libellé;Montant\n05/11/2026;A;-1,00\nhier;B;-2,00\n";
        let error = parse(csv).unwrap_err();
        assert!(error.starts_with("Ligne 3 :"), "{error}");
        assert!(error.contains("hier"));
    }

    #[test]
    fn refuse_un_montant_illisible_en_citant_la_ligne() {
        let csv = "Date;Libellé;Montant\n05/11/2026;A;beaucoup\n";
        let error = parse(csv).unwrap_err();
        assert!(error.starts_with("Ligne 2 :"), "{error}");
    }

    #[test]
    fn refuse_un_fichier_sans_en_tete_reconnaissable() {
        let error = parse("a;b;c\n1;2;3\n").unwrap_err();
        assert!(error.contains("en-tête"), "{error}");
    }

    #[test]
    fn refuse_un_fichier_vide_ou_sans_operation() {
        assert!(parse("").is_err());
        assert!(
            parse("Date;Libellé;Montant\n")
                .unwrap_err()
                .contains("aucune opération")
        );
    }
}
