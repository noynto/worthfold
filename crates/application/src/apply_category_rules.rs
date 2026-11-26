use std::sync::Arc;

use category::CategoryProvider;
use transaction::TransactionProvider;

use crate::UseCaseError;

pub struct ApplyCategoryRules {
    pub category_provider: Arc<dyn CategoryProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
}

pub struct Command;

pub struct Output {
    /// Les transactions sans catégorie que les règles ont classées.
    pub classified: usize,
}

impl ApplyCategoryRules {
    /// Classe, d'après les règles, toutes les transactions qui n'ont pas de catégorie.
    /// Une transaction déjà classée (à la main ou par une règle) n'est jamais touchée.
    pub async fn handle(&self, _command: Command) -> Result<Output, UseCaseError> {
        let rules = self.category_provider.read_rules().await?;
        let transactions = self.transaction_provider.read_all().await?;
        let assignments = service::transactions::rule_assignments(&transactions, &rules);
        self.transaction_provider
            .set_categories(&assignments)
            .await?;
        Ok(Output {
            classified: assignments.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use category::CategoryRule;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{CategoryRuleId, Currency, Money, TransactionId};
    use testing::{InMemoryCategories, InMemoryTransactions};
    use transaction::{Transaction, TransactionSource};

    fn transaction(label: &str, category: Option<&str>) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: "a".into(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            amount: Money::new(Decimal::ONE, Currency::Eur),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: category.map(Into::into),
        }
    }

    #[tokio::test]
    async fn classe_les_transactions_sans_categorie_et_respecte_les_choix_existants() {
        let rule = CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: "logement".into(),
            pattern: "loyer".into(),
        };
        let free = transaction("PRLV LOYER", None);
        let chosen = transaction("PRLV LOYER", Some("autre"));
        let transactions = Arc::new(InMemoryTransactions::with([free.clone(), chosen.clone()]));

        let output = ApplyCategoryRules {
            category_provider: Arc::new(InMemoryCategories::with([], [rule])),
            transaction_provider: transactions.clone(),
        }
        .handle(Command)
        .await
        .unwrap();

        assert_eq!(output.classified, 1);
        assert_eq!(
            transactions
                .read(&free.id)
                .await
                .unwrap()
                .unwrap()
                .category_id,
            Some("logement".into())
        );
        assert_eq!(
            transactions
                .read(&chosen.id)
                .await
                .unwrap()
                .unwrap()
                .category_id,
            Some("autre".into())
        );
    }

    #[tokio::test]
    async fn sans_regle_rien_ne_change() {
        let output = ApplyCategoryRules {
            category_provider: Arc::new(InMemoryCategories::default()),
            transaction_provider: Arc::new(InMemoryTransactions::with([transaction("x", None)])),
        }
        .handle(Command)
        .await
        .unwrap();
        assert_eq!(output.classified, 0);
    }
}
