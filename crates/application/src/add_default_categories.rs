use std::sync::Arc;

use category::{Category, CategoryProvider};
use shared::CategoryId;

use crate::UseCaseError;

pub struct AddDefaultCategories {
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Command;

pub struct Output {
    /// Les catégories usuelles créées. Celles dont le nom existait déjà sont laissées telles quelles.
    pub added: usize,
}

impl AddDefaultCategories {
    pub async fn handle(&self, _command: Command) -> Result<Output, UseCaseError> {
        let existing = self.category_provider.read_all().await?;
        let mut added = 0;
        for (name, kind) in Category::DEFAULTS {
            if existing.iter().any(|c| c.same_name(name)) {
                continue;
            }
            self.category_provider
                .write(&Category {
                    id: CategoryId::generate(),
                    name: name.to_owned(),
                    kind,
                })
                .await?;
            added += 1;
        }
        Ok(Output { added })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use category::CategoryKind;
    use testing::InMemoryCategories;

    #[tokio::test]
    async fn cree_les_categories_usuelles_une_seule_fois() {
        let provider = Arc::new(InMemoryCategories::default());
        let use_case = AddDefaultCategories {
            category_provider: provider.clone(),
        };
        let first = use_case.handle(Command).await.unwrap();
        let second = use_case.handle(Command).await.unwrap();

        assert_eq!(first.added, Category::DEFAULTS.len());
        assert_eq!(second.added, 0);
        assert_eq!(
            provider.read_all().await.unwrap().len(),
            Category::DEFAULTS.len()
        );
    }

    #[tokio::test]
    async fn ne_recree_pas_une_categorie_existante() {
        let existing = Category {
            id: CategoryId::generate(),
            name: "courses".into(),
            kind: CategoryKind::Expense,
        };
        let provider = Arc::new(InMemoryCategories::with([existing], []));
        let output = AddDefaultCategories {
            category_provider: provider.clone(),
        }
        .handle(Command)
        .await
        .unwrap();
        assert_eq!(output.added, Category::DEFAULTS.len() - 1);
    }
}
