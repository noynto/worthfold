//! Lecture d'un export CSV de relevé bancaire : une fonction pure, des octets et le format de la banque en
//! entrée, des lignes en sortie.
//!
//! Le format ([`CsvFormat`]) est décrit une fois par banque : encodage, séparateur, ligne d'en-tête, colonnes
//! (désignées par leur nom, sans tenir compte de la casse ni des accents), écriture des dates et des montants.
//! Rien n'est deviné : un fichier qui ne correspond pas est refusé en entier, avec la raison.

use chrono::NaiveDate;
use institution::{AmountColumns, CsvFormat, DecimalSeparator, Encoding};
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
pub fn parse(bytes: &[u8], format: &CsvFormat) -> Result<Vec<StatementLine>, String> {
    let text = decode(bytes, format.encoding)?;
    // Les lignes avant l'en-tête (nom du compte, période…) ne font pas partie du tableau.
    let skipped = format.header_line.saturating_sub(1) as usize;
    let table: String = text.split_inclusive('\n').skip(skipped).collect();
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(format.delimiter.byte())
        .has_headers(false)
        .flexible(true)
        .from_reader(table.as_bytes());

    let mut columns: Option<Columns> = None;
    let mut lines = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("Le fichier n'est pas un CSV lisible : {e}."))?;
        let fields: Vec<&str> = record.iter().collect();
        if fields.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let line_number = record.position().map_or(0, |p| p.line()) + skipped as u64;
        let Some(columns) = &columns else {
            columns = Some(find_columns(&fields, format, line_number)?);
            continue;
        };
        if let Some(line) = read_line(columns, format, &fields, line_number)? {
            lines.push(line);
        }
    }
    if columns.is_none() {
        return Err(format!(
            "Le fichier est vide : je ne trouve pas l'en-tête à la ligne {}.",
            format.header_line
        ));
    }
    if lines.is_empty() {
        return Err("Le relevé ne contient aucune opération.".into());
    }
    Ok(lines)
}

fn decode(bytes: &[u8], encoding: Encoding) -> Result<String, String> {
    match encoding {
        Encoding::Utf8 => {
            let text = std::str::from_utf8(bytes).map_err(|_| {
                "Le fichier n'est pas de l'UTF-8 : change l'encodage dans les réglages de la banque \
                 (Windows-1252 pour beaucoup de banques françaises)."
                    .to_owned()
            })?;
            Ok(text.trim_start_matches('\u{feff}').to_owned())
        }
        // Windows-1252 est identique à Latin-1, sauf le signe euro (0x80).
        Encoding::Windows1252 => Ok(bytes
            .iter()
            .map(|&b| if b == 0x80 { '€' } else { char::from(b) })
            .collect()),
    }
}

/// Minuscules et sans accents, pour comparer des en-têtes écrits de dix façons.
pub(crate) fn normalize(header: &str) -> String {
    header
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn find_columns(fields: &[&str], format: &CsvFormat, line: u64) -> Result<Columns, String> {
    let headers: Vec<String> = fields.iter().map(|f| normalize(f)).collect();
    let find = |wanted: &str| -> Result<usize, String> {
        let normalized = normalize(wanted);
        headers.iter().position(|h| *h == normalized).ok_or_else(|| {
            let found: Vec<String> = fields
                .iter()
                .map(|f| f.trim())
                .filter(|f| !f.is_empty())
                .map(|f| format!("« {f} »"))
                .collect();
            format!(
                "La colonne « {wanted} » est introuvable à la ligne {line}. Colonnes lues : {}. \
                 Vérifie le séparateur et la ligne d'en-tête dans les réglages de la banque.",
                if found.is_empty() {
                    "aucune".to_owned()
                } else {
                    found.join(", ")
                }
            )
        })
    };
    Ok(Columns {
        date: find(&format.date_column)?,
        label: find(&format.label_column)?,
        amount: match &format.amount {
            AmountColumns::Signed(column) => Amount::Signed(find(column)?),
            AmountColumns::DebitCredit { debit, credit } => Amount::DebitCredit {
                debit: find(debit)?,
                credit: find(credit)?,
            },
        },
    })
}

fn read_line(
    columns: &Columns,
    format: &CsvFormat,
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

    let date = NaiveDate::parse_from_str(field(columns.date), format.date_format.pattern())
        .map_err(|_| {
            format!(
                "Ligne {line_number} : la date « {} » n'est pas au format {}.",
                field(columns.date),
                format.date_format.label()
            )
        })?;
    let not_a_number = || {
        format!(
            "Ligne {line_number} : le montant « {} » n'est pas un nombre.",
            amount_text.join(" ").trim()
        )
    };
    let separator = format.decimal_separator;
    let amount = match columns.amount {
        Amount::Signed(_) => {
            let amount = parse_amount(amount_text[0], separator).ok_or_else(not_a_number)?;
            if format.invert_sign { -amount } else { amount }
        }
        Amount::DebitCredit { .. } => {
            // Une colonne débit est toujours une dépense, qu'elle soit saisie en positif ou en négatif.
            let debit = parse_optional_amount(amount_text[0], separator)
                .ok_or_else(not_a_number)?
                .map(|d| -d.abs());
            let credit = parse_optional_amount(amount_text[1], separator)
                .ok_or_else(not_a_number)?
                .map(|c| c.abs());
            match (debit, credit) {
                (Some(d), None) => d,
                (None, Some(c)) => c,
                (Some(d), Some(c)) => d + c,
                (None, None) => return Err(not_a_number()),
            }
        }
    };
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

/// `Some(None)` pour une cellule vide (la colonne débit d'un crédit, par exemple) ; `None` si ce n'est pas un nombre.
fn parse_optional_amount(text: &str, separator: DecimalSeparator) -> Option<Option<Decimal>> {
    if text.is_empty() {
        return Some(None);
    }
    parse_amount(text, separator).map(Some)
}

/// Un montant avec son séparateur décimal : l'autre séparateur et les espaces (milliers), le signe euro
/// et les lettres sont ignorés ; le signe moins typographique est accepté.
pub(crate) fn parse_amount(text: &str, separator: DecimalSeparator) -> Option<Decimal> {
    let (decimal, thousands) = match separator {
        DecimalSeparator::Comma => (',', '.'),
        DecimalSeparator::Dot => ('.', ','),
    };
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '€' && *c != thousands && !c.is_alphabetic())
        .map(|c| match c {
            '−' => '-',
            c if c == decimal => '.',
            c => c,
        })
        .collect();
    cleaned.parse::<Decimal>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use institution::{DateFormat, Delimiter};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn signed() -> CsvFormat {
        CsvFormat::suggested()
    }

    fn debit_credit() -> CsvFormat {
        CsvFormat {
            amount: AmountColumns::DebitCredit {
                debit: "Débit".into(),
                credit: "Crédit".into(),
            },
            ..CsvFormat::suggested()
        }
    }

    #[test]
    fn lit_un_releve_a_montant_signe_en_format_francais() {
        let csv = "Date;Libellé;Montant\n\
                   05/11/2026;PRLV LOYER  NOVEMBRE;-700,00\n\
                   06/11/2026;VIR SALAIRE;2 150,50\n";
        let lines = parse(csv.as_bytes(), &signed()).unwrap();
        assert_eq!(
            lines,
            vec![
                StatementLine {
                    date: day(2026, 11, 5),
                    label: "PRLV LOYER NOVEMBRE".into(),
                    amount: Decimal::new(-70_000, 2),
                },
                StatementLine {
                    date: day(2026, 11, 6),
                    label: "VIR SALAIRE".into(),
                    amount: Decimal::new(215_050, 2),
                },
            ]
        );
    }

    #[test]
    fn reconnait_les_colonnes_a_leur_nom_sans_casse_ni_accent() {
        let csv = "DATE;LIBELLE;montant\n05/11/2026;Café;-3,50\n";
        assert_eq!(parse(csv.as_bytes(), &signed()).unwrap().len(), 1);
    }

    #[test]
    fn ignore_les_lignes_avant_l_en_tete_et_les_pieds_de_page() {
        let csv = "Compte courant;FR76…\nDu 01/11 au 30/11;\nDate;Libellé;Montant\n\
                   05/11/2026;Loyer;-700,00\n;;\nSolde;;1 250,00\n";
        let format = CsvFormat {
            header_line: 3,
            ..signed()
        };
        // « Solde » n'a pas de date mais a un montant : c'est une ligne illisible, pas un pied de page muet.
        assert!(parse(csv.as_bytes(), &format).is_err());
        let csv = "Compte courant;FR76…\nDu 01/11 au 30/11;\nDate;Libellé;Montant\n\
                   05/11/2026;Loyer;-700,00\n;;\n";
        assert_eq!(parse(csv.as_bytes(), &format).unwrap().len(), 1);
    }

    #[test]
    fn dit_a_quelle_ligne_une_colonne_manque_et_ce_qui_a_ete_lu() {
        let csv = "Date;Intitulé;Montant\n05/11/2026;Loyer;-700,00\n";
        let error = parse(csv.as_bytes(), &signed()).unwrap_err();
        assert!(error.contains("« Libellé »"), "{error}");
        assert!(error.contains("ligne 1"), "{error}");
        assert!(error.contains("« Intitulé »"), "{error}");
    }

    #[test]
    fn lit_deux_colonnes_debit_et_credit() {
        let csv = "Date;Libellé;Débit;Crédit\n\
                   05/11/2026;Loyer;700,00;\n\
                   06/11/2026;Salaire;;2 000,00\n\
                   07/11/2026;Remboursement;-5,00;\n";
        let amounts: Vec<Decimal> = parse(csv.as_bytes(), &debit_credit())
            .unwrap()
            .iter()
            .map(|l| l.amount)
            .collect();
        assert_eq!(
            amounts,
            vec![
                Decimal::new(-70_000, 2),
                Decimal::new(200_000, 2),
                Decimal::new(-500, 2)
            ]
        );
    }

    #[test]
    fn inverse_le_signe_d_une_banque_qui_note_les_depenses_en_positif() {
        let csv = "Date;Libellé;Montant\n05/11/2026;Café;3,50\n";
        let format = CsvFormat {
            invert_sign: true,
            ..signed()
        };
        assert_eq!(
            parse(csv.as_bytes(), &format).unwrap()[0].amount,
            Decimal::new(-350, 2)
        );
    }

    #[test]
    fn lit_un_format_anglo_saxon_a_virgule_de_colonne_et_point_decimal() {
        let csv = "Date,Libellé,Montant\n2026-11-05,Café,\"-1,234.50\"\n";
        let format = CsvFormat {
            delimiter: Delimiter::Comma,
            date_format: DateFormat::YearMonthDay,
            decimal_separator: DecimalSeparator::Dot,
            ..signed()
        };
        let lines = parse(csv.as_bytes(), &format).unwrap();
        assert_eq!(lines[0].amount, Decimal::new(-123_450, 2));
        assert_eq!(lines[0].date, day(2026, 11, 5));
    }

    #[test]
    fn lit_le_windows_1252_et_refuse_de_l_utf8_faux() {
        // « Libellé;Montant € » : é = 0xE9.
        let windows = [
            &b"Date;Libell"[..],
            &[0xE9],
            &b";Montant\n05/11/2026;Caf"[..],
            &[0xE9],
            &b";-3,50 "[..],
            &[0x80, b'\n'],
        ]
        .concat();
        let format = CsvFormat {
            encoding: Encoding::Windows1252,
            ..signed()
        };
        let lines = parse(&windows, &format).unwrap();
        assert_eq!(lines[0].label, "Café");
        // Le même fichier lu comme de l'UTF-8 est refusé, avec le conseil de changer l'encodage.
        assert!(parse(&windows, &signed()).unwrap_err().contains("UTF-8"));
    }

    #[test]
    fn accepte_le_bom_utf8_et_le_moins_typographique() {
        let csv = "\u{feff}Date;Libellé;Montant\n05/11/2026;Café;−3,50\n";
        assert_eq!(
            parse(csv.as_bytes(), &signed()).unwrap()[0].amount,
            Decimal::new(-350, 2)
        );
    }

    #[test]
    fn refuse_une_date_au_mauvais_format_avec_la_ligne() {
        let csv = "Date;Libellé;Montant\n05/11/2026;Loyer;-700,00\n2026-11-06;Café;-3,50\n";
        let error = parse(csv.as_bytes(), &signed()).unwrap_err();
        assert!(error.contains("Ligne 3"), "{error}");
        assert!(error.contains("jj/mm/aaaa"), "{error}");
    }

    #[test]
    fn refuse_un_montant_illisible_et_un_fichier_sans_operation() {
        let csv = "Date;Libellé;Montant\n05/11/2026;Loyer;beaucoup\n";
        assert!(
            parse(csv.as_bytes(), &signed())
                .unwrap_err()
                .contains("Ligne 2")
        );
        assert!(
            parse(b"Date;Libell\xc3\xa9;Montant\n", &signed())
                .unwrap_err()
                .contains("aucune opération")
        );
        assert!(parse(b"", &signed()).is_err());
    }

    /// Un export réel du Crédit Agricole (anonymisé) : préambule, libellés sur plusieurs lignes, colonnes
    /// débit et crédit, point-virgule final, fins de ligne Windows.
    #[test]
    fn lit_un_export_du_credit_agricole() {
        let csv = "Téléchargement du 27/11/2026;\r\n\r\n\r\nM.       DUPONT JEAN\r\n\r\n\
                   Livret A carte n° 123\r\n\r\nSolde au 27/11/2026 10,14 €\r\n\r\n\
                   Liste des opérations du compte entre le 01/11/2025 et le 27/11/2026;\r\n\r\n\
                   Date d'opération;Date valeur;Libellé;Débit euros;Crédit euros;\r\n\
                   21/11/2026;16/11/2026;\"Virement émis\r\nWEB M. DUPONT JEAN\r\n\r\n\r\n\";250,00;;\r\n\
                   03/11/2026;01/11/2026;\"Virement reçu\r\nSALAIRE\";;1 200,00;\r\n";
        let format = CsvFormat {
            header_line: 11,
            date_column: "Date d'opération".into(),
            amount: AmountColumns::DebitCredit {
                debit: "Débit euros".into(),
                credit: "Crédit euros".into(),
            },
            ..CsvFormat::suggested()
        };
        let lines = parse(csv.as_bytes(), &format).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].date, day(2026, 11, 21));
        assert_eq!(lines[0].label, "Virement émis WEB M. DUPONT JEAN");
        assert_eq!(lines[0].amount, Decimal::new(-25_000, 2));
        assert_eq!(lines[1].amount, Decimal::new(120_000, 2));
    }
}
