use std::sync::Arc;

use category::{Category, CategoryKind, CategoryProvider};
use shared::CategoryId;

use crate::UseCaseError;

pub struct AddCategory {
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Command {
    pub name: String,
    pub kind: CategoryKind,
}

pub struct Output {
    pub category: Category,
}

impl AddCategory {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let name = Category::validate_name(&command.name).map_err(UseCaseError::refusal)?;
        // Deux catégories de même nom seraient indiscernables dans les listes.
        let existing = self.category_provider.read_all().await?;
        if existing.iter().any(|c| c.same_name(&name)) {
            return Err(UseCaseError::refusal(format!(
                "La catégorie « {name} » existe déjà."
            )));
        }
        let category = Category {
            id: CategoryId::generate(),
            name,
            kind: command.kind,
        };
        self.category_provider.write(&category).await?;
        Ok(Output { category })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::InMemoryCategories;

    fn use_case() -> (AddCategory, Arc<InMemoryCategories>) {
        let provider = Arc::new(InMemoryCategories::default());
        (
            AddCategory {
                category_provider: provider.clone(),
            },
            provider,
        )
    }

    fn command(name: &str) -> Command {
        Command {
            name: name.into(),
            kind: CategoryKind::Expense,
        }
    }

    #[tokio::test]
    async fn ajoute_une_categorie_au_nom_nettoye() {
        let (use_case, provider) = use_case();
        let output = use_case.handle(command("  Courses ")).await.unwrap();
        assert_eq!(output.category.name, "Courses");
        assert_eq!(provider.read_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuse_un_nom_vide() {
        let (use_case, _) = use_case();
        assert!(matches!(
            use_case.handle(command("  ")).await,
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[tokio::test]
    async fn refuse_un_nom_deja_pris_sans_tenir_compte_de_la_casse() {
        let (use_case, provider) = use_case();
        use_case.handle(command("Énergie")).await.unwrap();
        assert!(matches!(
            use_case.handle(command("energie")).await,
            Err(UseCaseError::Refusal(_))
        ));
        assert_eq!(provider.read_all().await.unwrap().len(), 1);
    }
}
