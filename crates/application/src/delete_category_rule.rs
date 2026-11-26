use std::sync::Arc;

use category::{CategoryProvider, CategoryRule};
use shared::CategoryRuleId;

use crate::UseCaseError;

pub struct DeleteCategoryRule {
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Command {
    pub rule_id: CategoryRuleId,
}

pub struct Output {
    pub rule: CategoryRule,
}

impl DeleteCategoryRule {
    /// Supprime la règle. Les transactions qu'elle a déjà classées gardent leur catégorie.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let rule = self
            .category_provider
            .read_rule(&command.rule_id)
            .await?
            .ok_or_else(|| {
                UseCaseError::refusal("Cette règle n'existe plus : elle a peut-être été supprimée.")
            })?;
        self.category_provider.delete_rule(&rule.id).await?;
        Ok(Output { rule })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::InMemoryCategories;

    #[tokio::test]
    async fn supprime_la_regle_visee_et_elle_seule() {
        let rule = |pattern: &str| CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: "c".into(),
            pattern: pattern.into(),
        };
        let (first, second) = (rule("a"), rule("b"));
        let provider = Arc::new(InMemoryCategories::with(
            [],
            [first.clone(), second.clone()],
        ));
        DeleteCategoryRule {
            category_provider: provider.clone(),
        }
        .handle(Command { rule_id: first.id })
        .await
        .unwrap();
        assert_eq!(provider.read_rules().await.unwrap(), vec![second]);
    }

    #[tokio::test]
    async fn refuse_une_regle_inconnue() {
        let use_case = DeleteCategoryRule {
            category_provider: Arc::new(InMemoryCategories::default()),
        };
        assert!(matches!(
            use_case
                .handle(Command {
                    rule_id: "x".into()
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
