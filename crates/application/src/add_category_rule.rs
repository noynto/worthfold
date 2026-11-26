use std::sync::Arc;

use category::{CategoryProvider, CategoryRule, normalize};
use shared::{CategoryId, CategoryRuleId};

use crate::{CATEGORY_NOT_FOUND, UseCaseError};

pub struct AddCategoryRule {
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Command {
    pub category_id: CategoryId,
    /// Le mot ou la suite de mots que le libellé doit contenir.
    pub pattern: String,
}

pub struct Output {
    pub rule: CategoryRule,
}

impl AddCategoryRule {
    /// Ajoute la règle. Elle ne reclasse rien d'elle-même : les transactions déjà enregistrées
    /// sont classées par `ApplyCategoryRules`, les prochaines à l'import.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let pattern =
            CategoryRule::validate_pattern(&command.pattern).map_err(UseCaseError::refusal)?;
        let category = self
            .category_provider
            .read(&command.category_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(CATEGORY_NOT_FOUND))?;
        // Un même motif ne peut mener qu'à une catégorie : sinon le résultat dépendrait de l'ordre de création.
        let rules = self.category_provider.read_rules().await?;
        if let Some(existing) = rules
            .iter()
            .find(|r| normalize(&r.pattern) == normalize(&pattern))
        {
            let message = if existing.category_id == category.id {
                format!(
                    "La règle « {pattern} » existe déjà pour « {} ».",
                    category.name
                )
            } else {
                let other = self
                    .category_provider
                    .read(&existing.category_id)
                    .await?
                    .map_or_else(
                        || "une autre catégorie".to_owned(),
                        |c| format!("« {} »", c.name),
                    );
                format!(
                    "Le motif « {pattern} » classe déjà dans {other} : supprime d'abord cette règle."
                )
            };
            return Err(UseCaseError::refusal(message));
        }
        let rule = CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: category.id,
            pattern,
        };
        self.category_provider.write_rule(&rule).await?;
        Ok(Output { rule })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use category::{Category, CategoryKind};
    use testing::InMemoryCategories;

    fn category(id: &str) -> Category {
        Category {
            id: id.into(),
            name: id.into(),
            kind: CategoryKind::Expense,
        }
    }

    fn setup() -> (AddCategoryRule, Arc<InMemoryCategories>) {
        let provider = Arc::new(InMemoryCategories::with([category("a"), category("b")], []));
        (
            AddCategoryRule {
                category_provider: provider.clone(),
            },
            provider,
        )
    }

    fn command(category: &str, pattern: &str) -> Command {
        Command {
            category_id: category.into(),
            pattern: pattern.into(),
        }
    }

    #[tokio::test]
    async fn ajoute_une_regle_au_motif_nettoye() {
        let (use_case, provider) = setup();
        let output = use_case.handle(command("a", "  Carrefour ")).await.unwrap();
        assert_eq!(output.rule.pattern, "Carrefour");
        assert_eq!(provider.read_rules().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuse_un_motif_vide_ou_une_categorie_inconnue() {
        let (use_case, _) = setup();
        assert!(matches!(
            use_case.handle(command("a", "  ")).await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(matches!(
            use_case.handle(command("inconnue", "x")).await,
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[tokio::test]
    async fn refuse_un_motif_deja_utilise_meme_pour_une_autre_categorie() {
        let (use_case, provider) = setup();
        use_case.handle(command("a", "Carrefour")).await.unwrap();
        for category in ["a", "b"] {
            assert!(matches!(
                use_case.handle(command(category, "  CARREFOUR")).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert_eq!(provider.read_rules().await.unwrap().len(), 1);
    }
}
