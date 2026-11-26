use std::sync::Arc;

use category::CategoryProvider;
use shared::{AssetId, CategoryId, TransactionId};
use transaction::{Transaction, TransactionProvider};

use crate::{CATEGORY_NOT_FOUND, UseCaseError};

pub struct CategorizeTransaction {
    pub category_provider: Arc<dyn CategoryProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
}

pub struct Command {
    /// Le compte dont la page envoie la demande : une transaction désignée depuis un autre compte
    /// (lien périmé ou forgé) est traitée comme absente.
    pub asset_id: AssetId,
    pub transaction_id: TransactionId,
    /// La catégorie choisie, ou `None` pour retirer celle de la transaction.
    pub category_id: Option<CategoryId>,
}

pub struct Output {
    pub transaction: Transaction,
    /// Le nom de la catégorie choisie, pour la confirmation.
    pub category_name: Option<String>,
}

impl CategorizeTransaction {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let mut transaction = self
            .transaction_provider
            .read(&command.transaction_id)
            .await?
            .filter(|t| t.asset_id == command.asset_id)
            .ok_or_else(|| {
                UseCaseError::refusal(
                    "Cette opération n'existe plus : elle a peut-être été supprimée avec son compte.",
                )
            })?;
        let category = match &command.category_id {
            Some(id) => Some(
                self.category_provider
                    .read(id)
                    .await?
                    .ok_or_else(|| UseCaseError::refusal(CATEGORY_NOT_FOUND))?,
            ),
            None => None,
        };
        self.transaction_provider
            .set_category(&transaction.id, category.as_ref().map(|c| &c.id))
            .await?;
        transaction.category_id = category.as_ref().map(|c| c.id.clone());
        Ok(Output {
            transaction,
            category_name: category.map(|c| c.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use category::{Category, CategoryKind};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::{InMemoryCategories, InMemoryTransactions};
    use transaction::TransactionSource;

    fn setup() -> (
        CategorizeTransaction,
        Arc<InMemoryTransactions>,
        Transaction,
    ) {
        let transaction = Transaction {
            id: TransactionId::generate(),
            asset_id: "a".into(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            amount: Money::new(Decimal::ONE, Currency::Eur),
            label: "x".into(),
            source: TransactionSource::Csv,
            category_id: None,
        };
        let transactions = Arc::new(InMemoryTransactions::with([transaction.clone()]));
        let category = Category {
            id: "c".into(),
            name: "Courses".into(),
            kind: CategoryKind::Expense,
        };
        (
            CategorizeTransaction {
                category_provider: Arc::new(InMemoryCategories::with([category], [])),
                transaction_provider: transactions.clone(),
            },
            transactions,
            transaction,
        )
    }

    fn command(transaction: &Transaction, asset: &str, category: Option<&str>) -> Command {
        Command {
            asset_id: asset.into(),
            transaction_id: transaction.id.clone(),
            category_id: category.map(Into::into),
        }
    }

    #[tokio::test]
    async fn classe_puis_declasse_une_transaction() {
        let (use_case, transactions, transaction) = setup();
        let output = use_case
            .handle(command(&transaction, "a", Some("c")))
            .await
            .unwrap();
        assert_eq!(output.category_name.as_deref(), Some("Courses"));
        assert_eq!(
            transactions
                .read(&transaction.id)
                .await
                .unwrap()
                .unwrap()
                .category_id,
            Some("c".into())
        );

        use_case
            .handle(command(&transaction, "a", None))
            .await
            .unwrap();
        assert_eq!(
            transactions
                .read(&transaction.id)
                .await
                .unwrap()
                .unwrap()
                .category_id,
            None
        );
    }

    #[tokio::test]
    async fn refuse_une_categorie_inconnue_sans_rien_changer() {
        let (use_case, transactions, transaction) = setup();
        assert!(matches!(
            use_case
                .handle(command(&transaction, "a", Some("inconnue")))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert_eq!(
            transactions
                .read(&transaction.id)
                .await
                .unwrap()
                .unwrap()
                .category_id,
            None
        );
    }

    #[tokio::test]
    async fn refuse_une_transaction_d_un_autre_compte_ou_inconnue() {
        let (use_case, _, transaction) = setup();
        assert!(matches!(
            use_case
                .handle(command(&transaction, "autre", Some("c")))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        let mut unknown = command(&transaction, "a", Some("c"));
        unknown.transaction_id = TransactionId::generate();
        assert!(matches!(
            use_case.handle(unknown).await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
