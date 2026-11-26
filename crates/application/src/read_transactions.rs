use std::sync::Arc;

use asset::{Asset, AssetProvider};
use category::{Category, CategoryProvider};
use shared::AssetId;
use transaction::{PendingTransaction, Transaction, TransactionProvider};

use crate::{NOT_FOUND, UseCaseError};

pub struct ReadTransactions {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
}

pub struct Query {
    pub asset_id: AssetId,
}

pub struct Output {
    pub asset: Asset,
    /// Les opérations du compte, de la plus récente à la plus ancienne
    /// (à date égale, la dernière importée d'abord).
    pub transactions: Vec<Transaction>,
    /// Ce que la banque annonce sans l'avoir encore comptabilisé, de la plus récente à la plus ancienne.
    /// Ces opérations ne sont ni classables ni comptées dans le budget : elles seront remplacées par les
    /// opérations comptabilisées à la prochaine lecture.
    pub pending: Vec<PendingTransaction>,
    /// Les catégories proposées pour classer une opération, par nom.
    pub categories: Vec<Category>,
}

impl ReadTransactions {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let asset = self
            .asset_provider
            .read(&query.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        // Le provider rend l'ordre d'import : l'inversion met la plus récente en tête.
        let mut transactions = self.transaction_provider.read_of_asset(&asset.id).await?;
        transactions.reverse();
        let mut pending: Vec<_> = self
            .transaction_provider
            .read_pending()
            .await?
            .into_iter()
            .filter(|p| p.asset_id == asset.id)
            .collect();
        pending.reverse();
        Ok(Output {
            asset,
            transactions,
            pending,
            categories: self.category_provider.read_all().await?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::AssetKind;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money, TransactionId};
    use testing::{InMemoryAssets, InMemoryCategories, InMemoryTransactions};
    use transaction::TransactionSource;

    fn transaction(asset: &str, label: &str, day: u32) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: asset.into(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            amount: Money::new(Decimal::from(-1), Currency::Eur),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: None,
        }
    }

    fn use_case(transactions: Vec<Transaction>) -> ReadTransactions {
        let asset = Asset {
            id: "a".into(),
            name: "Compte courant".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        ReadTransactions {
            asset_provider: Arc::new(InMemoryAssets::with([asset])),
            transaction_provider: Arc::new(InMemoryTransactions::with(transactions)),
            category_provider: Arc::new(InMemoryCategories::default()),
        }
    }

    #[tokio::test]
    async fn rend_les_operations_du_compte_de_la_plus_recente_a_la_plus_ancienne() {
        let output = use_case(vec![
            transaction("a", "ancienne", 3),
            transaction("a", "récente", 9),
            transaction("b", "autre compte", 5),
        ])
        .handle(Query {
            asset_id: "a".into(),
        })
        .await
        .unwrap();

        let labels: Vec<_> = output
            .transactions
            .iter()
            .map(|t| t.label.as_str())
            .collect();
        assert_eq!(labels, ["récente", "ancienne"]);
    }

    #[tokio::test]
    async fn refuse_un_compte_inconnu() {
        let result = use_case(vec![])
            .handle(Query {
                asset_id: "x".into(),
            })
            .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }
}
