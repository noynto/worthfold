//! Le format CSV d'une banque tel que le donne un formulaire : des codes et du texte, vérifiés ici.

use institution::{
    AmountColumns, CsvFormat, DateFormat, DaySeparator, DecimalSeparator, Delimiter, Encoding,
    PdfFormat,
};

use crate::UseCaseError;

/// Les réglages saisis, avant vérification.
#[derive(Debug, Clone, Default)]
pub struct FormatInput {
    pub encoding: String,
    pub delimiter: String,
    pub header_line: String,
    pub date_column: String,
    pub date_format: String,
    pub label_column: String,
    /// `signed` (une colonne de montant) ou `debit_credit` (deux colonnes).
    pub amount_mode: String,
    pub amount_column: String,
    pub debit_column: String,
    pub credit_column: String,
    pub decimal_separator: String,
    pub invert_sign: bool,
    /// Le relevé PDF : la banque en fournit-elle, et comment le lire.
    pub pdf_enabled: bool,
    pub pdf_date_separator: String,
    pub pdf_debit_header: String,
    pub pdf_credit_header: String,
    pub pdf_balance_marker: String,
    pub pdf_end_marker: String,
}

fn code<T>(value: &str, what: &str, parse: fn(&str) -> Option<T>) -> Result<T, UseCaseError> {
    parse(value.trim()).ok_or_else(|| UseCaseError::refusal(format!("{what} : choix invalide.")))
}

fn column(value: &str, what: &str) -> Result<String, UseCaseError> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return Err(UseCaseError::refusal(format!(
            "{what} : indique le nom de la colonne, tel qu'il est écrit dans l'en-tête du fichier."
        )));
    }
    if value.chars().count() > CsvFormat::COLUMN_MAX_LENGTH {
        return Err(UseCaseError::refusal(format!(
            "{what} : {} caractères au plus.",
            CsvFormat::COLUMN_MAX_LENGTH
        )));
    }
    Ok(value)
}

/// Le texte de repère d'un relevé PDF (en-tête de colonne, début d'une ligne de solde…).
fn marker(value: &str, what: &str) -> Result<String, UseCaseError> {
    column(value, what)
}

/// Le format PDF, s'il est demandé.
pub(crate) fn pdf_format(input: &FormatInput) -> Result<Option<PdfFormat>, UseCaseError> {
    if !input.pdf_enabled {
        return Ok(None);
    }
    Ok(Some(PdfFormat {
        date_separator: code(
            &input.pdf_date_separator,
            "Date du relevé PDF",
            DaySeparator::from_code,
        )?,
        debit_header: marker(&input.pdf_debit_header, "PDF, en-tête des débits")?,
        credit_header: marker(&input.pdf_credit_header, "PDF, en-tête des crédits")?,
        balance_marker: marker(&input.pdf_balance_marker, "PDF, début des lignes de solde")?,
        end_marker: marker(&input.pdf_end_marker, "PDF, fin du tableau")?,
    }))
}

pub(crate) fn csv_format(input: &FormatInput) -> Result<CsvFormat, UseCaseError> {
    let header_line: u32 = input
        .header_line
        .trim()
        .parse()
        .ok()
        .filter(|n| (1..=CsvFormat::MAX_HEADER_LINE).contains(n))
        .ok_or_else(|| {
            UseCaseError::refusal(format!(
                "Ligne de l'en-tête : un nombre de 1 à {}.",
                CsvFormat::MAX_HEADER_LINE
            ))
        })?;
    let amount = match input.amount_mode.as_str() {
        "signed" => AmountColumns::Signed(column(&input.amount_column, "Colonne du montant")?),
        "debit_credit" => AmountColumns::DebitCredit {
            debit: column(&input.debit_column, "Colonne du débit")?,
            credit: column(&input.credit_column, "Colonne du crédit")?,
        },
        _ => return Err(UseCaseError::refusal("Montant : choix invalide.")),
    };
    Ok(CsvFormat {
        encoding: code(&input.encoding, "Encodage", Encoding::from_code)?,
        delimiter: code(&input.delimiter, "Séparateur", Delimiter::from_code)?,
        header_line,
        date_column: column(&input.date_column, "Colonne de la date")?,
        date_format: code(&input.date_format, "Format de date", DateFormat::from_code)?,
        label_column: column(&input.label_column, "Colonne du libellé")?,
        amount,
        decimal_separator: code(
            &input.decimal_separator,
            "Séparateur décimal",
            DecimalSeparator::from_code,
        )?,
        invert_sign: input.invert_sign,
    })
}

/// Un formulaire bien rempli, pour les tests.
#[cfg(test)]
pub(crate) fn valid_input() -> FormatInput {
    FormatInput {
        encoding: "utf8".into(),
        delimiter: "semicolon".into(),
        header_line: "1".into(),
        date_column: "Date".into(),
        date_format: "dmy_slash".into(),
        label_column: "Libellé".into(),
        amount_mode: "signed".into(),
        amount_column: "Montant".into(),
        decimal_separator: "comma".into(),
        pdf_date_separator: "dot".into(),
        pdf_debit_header: "Débit".into(),
        pdf_credit_header: "Crédit".into(),
        pdf_balance_marker: "SOLDE".into(),
        pdf_end_marker: "TOTAL DES OPERATIONS".into(),
        ..FormatInput::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construit_le_format_d_un_formulaire_valide() {
        let format = csv_format(&valid_input()).unwrap();
        assert_eq!(format, CsvFormat::suggested());
    }

    #[test]
    fn le_format_pdf_n_existe_que_si_la_banque_en_fournit() {
        assert_eq!(pdf_format(&valid_input()).unwrap(), None);
        let input = FormatInput {
            pdf_enabled: true,
            ..valid_input()
        };
        assert_eq!(pdf_format(&input).unwrap(), Some(PdfFormat::suggested()));
        let incomplete = FormatInput {
            pdf_debit_header: " ".into(),
            ..input
        };
        assert!(matches!(
            pdf_format(&incomplete),
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[test]
    fn lit_deux_colonnes_debit_et_credit() {
        let input = FormatInput {
            amount_mode: "debit_credit".into(),
            debit_column: " Débit ".into(),
            credit_column: "Crédit".into(),
            ..valid_input()
        };
        assert_eq!(
            csv_format(&input).unwrap().amount,
            AmountColumns::DebitCredit {
                debit: "Débit".into(),
                credit: "Crédit".into()
            }
        );
    }

    #[test]
    fn refuse_ce_qui_est_incomplet_ou_invalide() {
        for broken in [
            FormatInput {
                header_line: "0".into(),
                ..valid_input()
            },
            FormatInput {
                header_line: "x".into(),
                ..valid_input()
            },
            FormatInput {
                date_column: "  ".into(),
                ..valid_input()
            },
            FormatInput {
                encoding: "ebcdic".into(),
                ..valid_input()
            },
            FormatInput {
                amount_mode: "debit_credit".into(),
                ..valid_input()
            },
            FormatInput {
                amount_column: String::new(),
                ..valid_input()
            },
        ] {
            assert!(
                matches!(csv_format(&broken), Err(UseCaseError::Refusal(_))),
                "{broken:?}"
            );
        }
    }
}
