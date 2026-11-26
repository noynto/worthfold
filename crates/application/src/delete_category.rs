use std::sync::Arc;

use budget::BudgetProvider;
use category::{Category, CategoryProvider};
use recurrence::RecurrenceProvider;
use shared::CategoryId;
use transaction::TransactionProvider;

use crate::{CATEGORY_NOT_FOUND, UseCaseError};

pub struct DeleteCategory {
    pub category_provider: Arc<dyn CategoryProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub budget_provider: Arc<dyn BudgetProvider>,
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Command {
    pub category_id: CategoryId,
}

pub struct Output {
    /// La catégorie telle qu'elle était, pour confirmer ce qui a été supprimé.
    pub category: Category,
}

impl DeleteCategory {
    /// Supprime la catégorie, ses règles et ses enveloppes. Les transactions qu'elle classait ne sont pas supprimées :
    /// elles redeviennent sans catégorie.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let category = self
            .category_provider
            .read(&command.category_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(CATEGORY_NOT_FOUND))?;
        // Les références d'abord : la base refuse de supprimer une catégorie encore utilisée.
        // Mêmes limites que `DeleteAsset` : plusieurs écritures, sans transaction commune.
        self.transaction_provider
            .clear_category(&category.id)
            .await?;
        self.recurrence_provider
            .clear_category(&category.id)
            .await?;
        self.budget_provider
            .delete_envelopes_of_category(&category.id)
            .await?;
        self.category_provider
            .delete_rules_of_category(&category.id)
            .await?;
        self.category_provider.delete(&category.id).await?;
        Ok(Output { category })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use category::{CategoryKind, CategoryRule};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money, TransactionId};
    use testing::{InMemoryBudget, InMemoryCategories, InMemoryRecurrence, InMemoryTransactions};
    use transaction::{Transaction, TransactionSource};

    #[tokio::test]
    async fn supprime_la_categorie_et_ses_regles_et_declasse_ses_transactions() {
        let category = |id: &str| Category {
            id: id.into(),
            name: id.into(),
            kind: CategoryKind::Expense,
        };
        let rule = |category: &str| CategoryRule {
            id: shared::CategoryRuleId::generate(),
            category_id: category.into(),
            pattern: "x".into(),
        };
        let transaction = |category: &str| Transaction {
            id: TransactionId::generate(),
            asset_id: "a".into(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            amount: Money::new(Decimal::ONE, Currency::Eur),
            label: "x".into(),
            source: TransactionSource::Csv,
            category_id: Some(category.into()),
        };
        let categories = Arc::new(InMemoryCategories::with(
            [category("x"), category("y")],
            [rule("x"), rule("y")],
        ));
        let transactions = Arc::new(InMemoryTransactions::with([
            transaction("x"),
            transaction("y"),
        ]));

        DeleteCategory {
            category_provider: categories.clone(),
            transaction_provider: transactions.clone(),
            budget_provider: Arc::new(InMemoryBudget::default()),
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
        }
        .handle(Command {
            category_id: "x".into(),
        })
        .await
        .unwrap();

        let remaining: Vec<_> = categories
            .read_all()
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(remaining, vec![CategoryId::from("y")]);
        let rules = categories.read_rules().await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].category_id, CategoryId::from("y"));
        // Les transactions restent, la catégorie supprimée en moins.
        let stored = transactions.read_all().await.unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored.iter().filter(|t| t.category_id.is_none()).count(), 1);
    }

    #[tokio::test]
    async fn refuse_une_categorie_inconnue() {
        let use_case = DeleteCategory {
            category_provider: Arc::new(InMemoryCategories::default()),
            transaction_provider: Arc::new(InMemoryTransactions::default()),
            budget_provider: Arc::new(InMemoryBudget::default()),
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
        };
        assert!(matches!(
            use_case
                .handle(Command {
                    category_id: "x".into()
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[tokio::test]
    async fn supprimer_une_categorie_emporte_ses_enveloppes() {
        let envelope = |category: &str| budget::Envelope {
            id: shared::EnvelopeId::generate(),
            category_id: category.into(),
            limit: Some(Money::new(Decimal::from(400), Currency::Eur)),
            from_month: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        };
        let category = |id: &str| Category {
            id: id.into(),
            name: id.into(),
            kind: CategoryKind::Expense,
        };
        let budget = Arc::new(InMemoryBudget::with([envelope("x"), envelope("y")], None));
        DeleteCategory {
            category_provider: Arc::new(InMemoryCategories::with(
                [category("x"), category("y")],
                [],
            )),
            transaction_provider: Arc::new(InMemoryTransactions::default()),
            budget_provider: budget.clone(),
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
        }
        .handle(Command {
            category_id: "x".into(),
        })
        .await
        .unwrap();

        let remaining = budget.read_envelopes().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].category_id, CategoryId::from("y"));
    }
}
