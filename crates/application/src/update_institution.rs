use std::sync::Arc;

use institution::{Institution, InstitutionProvider};

use crate::UseCaseError;
use crate::institution_format::{FormatInput, csv_format, pdf_format};

pub const NOT_FOUND: &str = "Cette banque n'existe plus : elle a peut-être été supprimée.";

pub struct UpdateInstitution {
    pub institution_provider: Arc<dyn InstitutionProvider>,
}

pub struct Command {
    /// Le nom de la banque, qui ne change pas : il relie les comptes à leur banque.
    pub name: String,
    pub format: FormatInput,
}

pub struct Output {
    pub institution: Institution,
}

impl UpdateInstitution {
    /// Remplace le format CSV de la banque. Les opérations déjà importées ne changent pas.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let mut institution = self
            .institution_provider
            .read(&command.name)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        institution.csv = csv_format(&command.format)?;
        institution.pdf = pdf_format(&command.format)?;
        self.institution_provider.write(&institution).await?;
        Ok(Output { institution })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::institution_format::valid_input;
    use institution::{CsvFormat, Encoding};
    use testing::InMemoryInstitutions;

    fn setup() -> (UpdateInstitution, Arc<InMemoryInstitutions>) {
        let provider = Arc::new(InMemoryInstitutions::with([Institution {
            name: "Ma banque".into(),
            csv: CsvFormat::suggested(),
            pdf: None,
        }]));
        (
            UpdateInstitution {
                institution_provider: provider.clone(),
            },
            provider,
        )
    }

    #[tokio::test]
    async fn remplace_le_format_en_gardant_le_nom() {
        let (use_case, provider) = setup();
        let mut format = valid_input();
        format.encoding = "windows1252".into();
        use_case
            .handle(Command {
                name: "Ma banque".into(),
                format,
            })
            .await
            .unwrap();
        let stored = provider.read("Ma banque").await.unwrap().unwrap();
        assert_eq!(stored.csv.encoding, Encoding::Windows1252);
    }

    #[tokio::test]
    async fn refuse_une_banque_inconnue_ou_un_format_invalide() {
        let (use_case, provider) = setup();
        assert!(matches!(
            use_case
                .handle(Command {
                    name: "Autre".into(),
                    format: valid_input()
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        let mut bad = valid_input();
        bad.header_line = "0".into();
        assert!(matches!(
            use_case
                .handle(Command {
                    name: "Ma banque".into(),
                    format: bad
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert_eq!(
            provider.read("Ma banque").await.unwrap().unwrap().csv,
            CsvFormat::suggested()
        );
    }
}
