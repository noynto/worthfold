use std::sync::Arc;

use institution::{Institution, InstitutionProvider};

use crate::UseCaseError;
use crate::institution_format::{FormatInput, csv_format, pdf_format};

pub struct AddInstitution {
    pub institution_provider: Arc<dyn InstitutionProvider>,
}

pub struct Command {
    pub name: String,
    pub format: FormatInput,
}

pub struct Output {
    pub institution: Institution,
}

impl AddInstitution {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let name = Institution::validate_name(&command.name).map_err(UseCaseError::refusal)?;
        let csv = csv_format(&command.format)?;
        let pdf = pdf_format(&command.format)?;
        let existing = self.institution_provider.read_all().await?;
        let institution = Institution { name, csv, pdf };
        // Deux banques de même nom seraient indiscernables dans les listes.
        if existing.iter().any(|i| institution.same_name(&i.name)) {
            return Err(UseCaseError::refusal(format!(
                "La banque « {} » existe déjà.",
                institution.name
            )));
        }
        self.institution_provider.write(&institution).await?;
        Ok(Output { institution })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::institution_format::valid_input;
    use testing::InMemoryInstitutions;

    fn command(name: &str) -> Command {
        Command {
            name: name.into(),
            format: valid_input(),
        }
    }

    fn use_case() -> (AddInstitution, Arc<InMemoryInstitutions>) {
        let provider = Arc::new(InMemoryInstitutions::default());
        (
            AddInstitution {
                institution_provider: provider.clone(),
            },
            provider,
        )
    }

    #[tokio::test]
    async fn ajoute_une_banque_avec_son_format() {
        let (use_case, provider) = use_case();
        let output = use_case.handle(command("  Ma  banque ")).await.unwrap();
        assert_eq!(output.institution.name, "Ma banque");
        assert_eq!(provider.read_all().await.unwrap(), vec![output.institution]);
    }

    #[tokio::test]
    async fn refuse_un_nom_vide_ou_deja_pris_sans_tenir_compte_de_la_casse() {
        let (use_case, provider) = use_case();
        use_case.handle(command("Ma banque")).await.unwrap();
        for name in ["  ", "ma BANQUE"] {
            assert!(matches!(
                use_case.handle(command(name)).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert_eq!(provider.read_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuse_un_format_invalide_sans_rien_enregistrer() {
        let (use_case, provider) = use_case();
        let mut bad = command("Ma banque");
        bad.format.date_column.clear();
        assert!(matches!(
            use_case.handle(bad).await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(provider.read_all().await.unwrap().is_empty());
    }
}
