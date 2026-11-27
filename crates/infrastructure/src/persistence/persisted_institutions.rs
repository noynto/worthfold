use anyhow::Context;
use async_trait::async_trait;
use institution::{
    AmountColumns, CsvFormat, DateFormat, DaySeparator, DecimalSeparator, Delimiter, Encoding,
    Institution, InstitutionProvider, PdfFormat,
};
use sqlx::{FromRow, PgPool};

pub struct PersistedInstitutions {
    pool: PgPool,
}

impl PersistedInstitutions {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLUMNS: &str = "name, encoding, delimiter, header_line, date_column, date_format, \
                       label_column, amount_column, debit_column, credit_column, decimal_separator, invert_sign, \
                       pdf_date_separator, pdf_debit_header, pdf_credit_header, pdf_balance_marker, pdf_end_marker";

#[derive(FromRow)]
struct InstitutionRow {
    name: String,
    encoding: String,
    delimiter: String,
    header_line: i32,
    date_column: String,
    date_format: String,
    label_column: String,
    amount_column: Option<String>,
    debit_column: Option<String>,
    credit_column: Option<String>,
    decimal_separator: String,
    invert_sign: bool,
    pdf_date_separator: Option<String>,
    pdf_debit_header: Option<String>,
    pdf_credit_header: Option<String>,
    pdf_balance_marker: Option<String>,
    pdf_end_marker: Option<String>,
}

fn coded<T>(value: &str, what: &str, parse: fn(&str) -> Option<T>) -> anyhow::Result<T> {
    parse(value).with_context(|| format!("{what} inconnu(e) en base : « {value} »."))
}

impl TryFrom<InstitutionRow> for Institution {
    type Error = anyhow::Error;

    fn try_from(row: InstitutionRow) -> anyhow::Result<Self> {
        let amount = match (row.amount_column, row.debit_column, row.credit_column) {
            (Some(column), None, None) => AmountColumns::Signed(column),
            (None, Some(debit), Some(credit)) => AmountColumns::DebitCredit { debit, credit },
            _ => anyhow::bail!(
                "Colonnes de montant incohérentes pour la banque {}.",
                row.name
            ),
        };
        let pdf = match (
            row.pdf_date_separator,
            row.pdf_debit_header,
            row.pdf_credit_header,
            row.pdf_balance_marker,
            row.pdf_end_marker,
        ) {
            (None, None, None, None, None) => None,
            (Some(separator), Some(debit), Some(credit), Some(balance), Some(end)) => {
                Some(PdfFormat {
                    date_separator: coded(
                        &separator,
                        "Séparateur de date",
                        DaySeparator::from_code,
                    )?,
                    debit_header: debit,
                    credit_header: credit,
                    balance_marker: balance,
                    end_marker: end,
                })
            }
            _ => anyhow::bail!("Format PDF incohérent pour la banque {}.", row.name),
        };
        Ok(Institution {
            pdf,
            csv: CsvFormat {
                encoding: coded(&row.encoding, "Encodage", Encoding::from_code)?,
                delimiter: coded(&row.delimiter, "Séparateur", Delimiter::from_code)?,
                header_line: u32::try_from(row.header_line)?,
                date_column: row.date_column,
                date_format: coded(&row.date_format, "Format de date", DateFormat::from_code)?,
                label_column: row.label_column,
                amount,
                decimal_separator: coded(
                    &row.decimal_separator,
                    "Séparateur décimal",
                    DecimalSeparator::from_code,
                )?,
                invert_sign: row.invert_sign,
            },
            name: row.name,
        })
    }
}

#[async_trait]
impl InstitutionProvider for PersistedInstitutions {
    async fn read_all(&self) -> anyhow::Result<Vec<Institution>> {
        sqlx::query_as::<_, InstitutionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM institutions ORDER BY lower(name), name"
        )))
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les banques.")?
        .into_iter()
        .map(Institution::try_from)
        .collect()
    }

    async fn read(&self, name: &str) -> anyhow::Result<Option<Institution>> {
        sqlx::query_as::<_, InstitutionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM institutions WHERE name = $1"
        )))
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .context("Impossible de lire la banque.")?
        .map(Institution::try_from)
        .transpose()
    }

    async fn write(&self, institution: &Institution) -> anyhow::Result<()> {
        let csv = &institution.csv;
        let (amount, debit, credit) = match &csv.amount {
            AmountColumns::Signed(column) => (Some(column), None, None),
            AmountColumns::DebitCredit { debit, credit } => (None, Some(debit), Some(credit)),
        };
        sqlx::query(
            "INSERT INTO institutions (name, encoding, delimiter, header_line, date_column, date_format, \
             label_column, amount_column, debit_column, credit_column, decimal_separator, invert_sign, \
             pdf_date_separator, pdf_debit_header, pdf_credit_header, pdf_balance_marker, pdf_end_marker) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
             ON CONFLICT (name) DO UPDATE SET encoding = excluded.encoding, delimiter = excluded.delimiter, \
             header_line = excluded.header_line, date_column = excluded.date_column, \
             date_format = excluded.date_format, label_column = excluded.label_column, \
             amount_column = excluded.amount_column, debit_column = excluded.debit_column, \
             credit_column = excluded.credit_column, decimal_separator = excluded.decimal_separator, \
             invert_sign = excluded.invert_sign, pdf_date_separator = excluded.pdf_date_separator, \
             pdf_debit_header = excluded.pdf_debit_header, pdf_credit_header = excluded.pdf_credit_header, \
             pdf_balance_marker = excluded.pdf_balance_marker, pdf_end_marker = excluded.pdf_end_marker",
        )
        .bind(&institution.name)
        .bind(csv.encoding.code())
        .bind(csv.delimiter.code())
        .bind(i32::try_from(csv.header_line)?)
        .bind(&csv.date_column)
        .bind(csv.date_format.code())
        .bind(&csv.label_column)
        .bind(amount)
        .bind(debit)
        .bind(credit)
        .bind(csv.decimal_separator.code())
        .bind(csv.invert_sign)
        .bind(institution.pdf.as_ref().map(|p| p.date_separator.code()))
        .bind(institution.pdf.as_ref().map(|p| &p.debit_header))
        .bind(institution.pdf.as_ref().map(|p| &p.credit_header))
        .bind(institution.pdf.as_ref().map(|p| &p.balance_marker))
        .bind(institution.pdf.as_ref().map(|p| &p.end_marker))
        .execute(&self.pool)
        .await
        .with_context(|| format!("Impossible d'enregistrer la banque {}.", institution.name))?;
        Ok(())
    }

    async fn delete(&self, name: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM institutions WHERE name = $1")
            .bind(name)
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer la banque {name}."))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedAssets;
    use crate::persistence::testdb::database;
    use asset::{Asset, AssetKind, AssetProvider};
    use shared::{AssetId, Currency};

    fn bank(name: &str, amount: AmountColumns) -> Institution {
        Institution {
            name: name.into(),
            csv: CsvFormat {
                amount,
                ..CsvFormat::suggested()
            },
            pdf: None,
        }
    }

    #[tokio::test]
    async fn ecrit_relit_remplace_et_supprime_une_banque() {
        let Some(db) = database().await else { return };
        let banks = PersistedInstitutions::new(db.pool.clone());
        let first = bank("Banque Postale", AmountColumns::Signed("Montant".into()));
        banks.write(&first).await.unwrap();
        let split = bank(
            "Crédit Agricole",
            AmountColumns::DebitCredit {
                debit: "Débit".into(),
                credit: "Crédit".into(),
            },
        );
        banks.write(&split).await.unwrap();
        assert_eq!(
            banks.read_all().await.unwrap(),
            vec![first.clone(), split.clone()]
        );
        assert_eq!(banks.read("Crédit Agricole").await.unwrap(), Some(split));

        let changed = Institution {
            csv: CsvFormat {
                encoding: Encoding::Windows1252,
                invert_sign: true,
                ..first.csv.clone()
            },
            pdf: Some(PdfFormat::suggested()),
            ..first
        };
        banks.write(&changed).await.unwrap();
        assert_eq!(banks.read("Banque Postale").await.unwrap(), Some(changed));

        banks.delete("Banque Postale").await.unwrap();
        assert_eq!(banks.read("Banque Postale").await.unwrap(), None);
        db.drop_db().await;
    }

    #[tokio::test]
    async fn la_base_refuse_de_supprimer_une_banque_encore_utilisee() {
        let Some(db) = database().await else { return };
        let banks = PersistedInstitutions::new(db.pool.clone());
        let assets = PersistedAssets::new(db.pool.clone());
        banks
            .write(&bank("Ma banque", AmountColumns::Signed("Montant".into())))
            .await
            .unwrap();
        assets
            .write(&Asset {
                id: AssetId::generate(),
                name: "Courant".into(),
                kind: AssetKind::BankAccount,
                currency: Currency::Eur,
                bank_name: Some("Ma banque".into()),
                account_number: None,
            })
            .await
            .unwrap();
        assert!(banks.delete("Ma banque").await.is_err());
        db.drop_db().await;
    }
}
