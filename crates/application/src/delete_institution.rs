use std::sync::Arc;

use asset::AssetProvider;
use institution::{Institution, InstitutionProvider};

use crate::UseCaseError;
use crate::update_institution::NOT_FOUND;

pub struct DeleteInstitution {
    pub institution_provider: Arc<dyn InstitutionProvider>,
    pub asset_provider: Arc<dyn AssetProvider>,
}

pub struct Command {
    pub name: String,
}

pub struct Output {
    pub institution: Institution,
}

impl DeleteInstitution {
    /// Refuse de supprimer une banque dont des comptes dépendent : leurs relevés ne se liraient plus.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let institution = self
            .institution_provider
            .read(&command.name)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        let users: Vec<String> = self
            .asset_provider
            .read_all()
            .await?
            .into_iter()
            .filter(|a| a.bank_name.as_deref() == Some(institution.name.as_str()))
            .map(|a| format!("« {} »", a.name))
            .collect();
        if !users.is_empty() {
            return Err(UseCaseError::refusal(format!(
                "« {} » est la banque de {} : change d'abord leur banque.",
                institution.name,
                users.join(", ")
            )));
        }
        self.institution_provider.delete(&institution.name).await?;
        Ok(Output { institution })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use institution::CsvFormat;
    use shared::Currency;
    use testing::{InMemoryAssets, InMemoryInstitutions};

    fn setup(bank_of_account: Option<&str>) -> (DeleteInstitution, Arc<InMemoryInstitutions>) {
        let institutions = Arc::new(InMemoryInstitutions::with([Institution {
            name: "Ma banque".into(),
            csv: CsvFormat::suggested(),
            pdf: None,
        }]));
        let asset = Asset {
            id: "a".into(),
            name: "Courant".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: bank_of_account.map(Into::into),
            account_number: None,
        };
        (
            DeleteInstitution {
                institution_provider: institutions.clone(),
                asset_provider: Arc::new(InMemoryAssets::with([asset])),
            },
            institutions,
        )
    }

    #[tokio::test]
    async fn supprime_une_banque_sans_compte() {
        let (use_case, institutions) = setup(None);
        use_case
            .handle(Command {
                name: "Ma banque".into(),
            })
            .await
            .unwrap();
        assert!(institutions.read_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn refuse_une_banque_dont_un_compte_depend() {
        let (use_case, institutions) = setup(Some("Ma banque"));
        let Err(UseCaseError::Refusal(message)) = use_case
            .handle(Command {
                name: "Ma banque".into(),
            })
            .await
        else {
            panic!("refus attendu");
        };
        assert!(message.contains("« Courant »"), "{message}");
        assert_eq!(institutions.read_all().await.unwrap().len(), 1);
    }
}
