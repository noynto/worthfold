use std::sync::Arc;

use category::{Category, CategoryProvider, CategoryRule};

use crate::UseCaseError;

pub struct ReadCategories {
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Query;

pub struct Output {
    /// Par nom.
    pub categories: Vec<Category>,
    /// De la plus ancienne à la plus récente.
    pub rules: Vec<CategoryRule>,
}

impl ReadCategories {
    pub async fn handle(&self, _query: Query) -> Result<Output, UseCaseError> {
        Ok(Output {
            categories: self.category_provider.read_all().await?,
            rules: self.category_provider.read_rules().await?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use category::CategoryKind;
    use testing::InMemoryCategories;

    #[tokio::test]
    async fn rend_les_categories_par_nom_et_les_regles() {
        let category = |name: &str| Category {
            id: name.into(),
            name: name.into(),
            kind: CategoryKind::Expense,
        };
        let output = ReadCategories {
            category_provider: Arc::new(InMemoryCategories::with(
                [category("Santé"), category("courses")],
                [],
            )),
        }
        .handle(Query)
        .await
        .unwrap();
        let names: Vec<_> = output.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["courses", "Santé"]);
        assert!(output.rules.is_empty());
    }
}
