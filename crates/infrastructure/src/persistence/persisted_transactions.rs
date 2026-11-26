use anyhow::Context;
use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{AssetId, CategoryId, Money, TransactionId};
use sqlx::{FromRow, PgPool};
use transaction::{PendingTransaction, Transaction, TransactionProvider, TransactionSource};

pub struct PersistedTransactions {
    pool: PgPool,
}

impl PersistedTransactions {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct TransactionRow {
    id: String,
    asset_id: String,
    booked_on: NaiveDate,
    amount: Decimal,
    currency: String,
    label: String,
    source: String,
    category_id: Option<String>,
}

impl TryFrom<TransactionRow> for Transaction {
    type Error = anyhow::Error;

    fn try_from(row: TransactionRow) -> anyhow::Result<Self> {
        Ok(Transaction {
            id: TransactionId::from(row.id),
            asset_id: AssetId::from(row.asset_id),
            booked_on: row.booked_on,
            amount: Money::new(row.amount, row.currency.parse()?),
            label: row.label,
            source: TransactionSource::from_code(&row.source)
                .with_context(|| format!("Source de transaction inconnue : {}.", row.source))?,
            category_id: row.category_id.map(CategoryId::from),
        })
    }
}

#[derive(FromRow)]
struct PendingRow {
    asset_id: String,
    booked_on: NaiveDate,
    amount: Decimal,
    currency: String,
    label: String,
}

impl TryFrom<PendingRow> for PendingTransaction {
    type Error = anyhow::Error;

    fn try_from(row: PendingRow) -> anyhow::Result<Self> {
        Ok(PendingTransaction {
            asset_id: AssetId::from(row.asset_id),
            booked_on: row.booked_on,
            amount: Money::new(row.amount, row.currency.parse()?),
            label: row.label,
        })
    }
}

#[async_trait]
impl TransactionProvider for PersistedTransactions {
    /// Par date, puis par ordre d'import (les identifiants UUID v7 sont triés dans le temps).
    async fn read_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<Vec<Transaction>> {
        sqlx::query_as::<_, TransactionRow>(
            "SELECT id, asset_id, booked_on, amount, currency, label, source, category_id \
             FROM transactions WHERE asset_id = $1 ORDER BY booked_on, id",
        )
        .bind(asset_id.as_str())
        .fetch_all(&self.pool)
        .await
        .with_context(|| format!("Impossible de lire les transactions du compte {asset_id}."))?
        .into_iter()
        .map(Transaction::try_from)
        .collect()
    }

    async fn read_all(&self) -> anyhow::Result<Vec<Transaction>> {
        sqlx::query_as::<_, TransactionRow>(
            "SELECT id, asset_id, booked_on, amount, currency, label, source, category_id \
             FROM transactions ORDER BY booked_on, id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les transactions.")?
        .into_iter()
        .map(Transaction::try_from)
        .collect()
    }

    async fn read(&self, id: &TransactionId) -> anyhow::Result<Option<Transaction>> {
        sqlx::query_as::<_, TransactionRow>(
            "SELECT id, asset_id, booked_on, amount, currency, label, source, category_id \
             FROM transactions WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Impossible de lire la transaction {id}."))?
        .map(Transaction::try_from)
        .transpose()
    }

    /// Toutes les lignes ou aucune : un import à moitié écrit serait impossible à rejouer sans doublons.
    async fn write_many(&self, transactions: &[Transaction]) -> anyhow::Result<()> {
        let mut db_transaction = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction de base de données.")?;
        for transaction in transactions {
            sqlx::query(
                "INSERT INTO transactions (id, asset_id, booked_on, amount, currency, label, source, category_id) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(transaction.id.as_str())
            .bind(transaction.asset_id.as_str())
            .bind(transaction.booked_on)
            .bind(transaction.amount.amount)
            .bind(transaction.amount.currency.code())
            .bind(&transaction.label)
            .bind(transaction.source.code())
            .bind(transaction.category_id.as_ref().map(CategoryId::as_str))
            .execute(&mut *db_transaction)
            .await
            .with_context(|| {
                format!(
                    "Impossible d'écrire une transaction du compte {}.",
                    transaction.asset_id
                )
            })?;
        }
        db_transaction
            .commit()
            .await
            .context("Impossible de valider l'écriture des transactions.")?;
        Ok(())
    }

    async fn set_category(
        &self,
        id: &TransactionId,
        category_id: Option<&CategoryId>,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE transactions SET category_id = $1 WHERE id = $2")
            .bind(category_id.map(CategoryId::as_str))
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de classer la transaction {id}."))?;
        Ok(())
    }

    async fn set_categories(
        &self,
        assignments: &[(TransactionId, CategoryId)],
    ) -> anyhow::Result<()> {
        let mut db_transaction = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction de base de données.")?;
        for (id, category_id) in assignments {
            sqlx::query("UPDATE transactions SET category_id = $1 WHERE id = $2")
                .bind(category_id.as_str())
                .bind(id.as_str())
                .execute(&mut *db_transaction)
                .await
                .with_context(|| format!("Impossible de classer la transaction {id}."))?;
        }
        db_transaction
            .commit()
            .await
            .context("Impossible de valider le classement des transactions.")?;
        Ok(())
    }

    async fn clear_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        sqlx::query("UPDATE transactions SET category_id = NULL WHERE category_id = $1")
            .bind(category_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de retirer la catégorie {category_id} des transactions.")
            })?;
        Ok(())
    }

    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM pending_transactions WHERE asset_id = $1")
            .bind(asset_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de supprimer les opérations en attente du compte {asset_id}.")
            })?;
        sqlx::query("DELETE FROM transactions WHERE asset_id = $1")
            .bind(asset_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de supprimer les transactions du compte {asset_id}.")
            })?;
        Ok(())
    }

    /// Toutes les lignes du compte remplacées d'un coup : une lecture à moitié écrite fausserait le solde disponible.
    async fn replace_pending(
        &self,
        asset_id: &AssetId,
        pending: &[PendingTransaction],
    ) -> anyhow::Result<()> {
        let mut db_transaction = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction de base de données.")?;
        sqlx::query("DELETE FROM pending_transactions WHERE asset_id = $1")
            .bind(asset_id.as_str())
            .execute(&mut *db_transaction)
            .await
            .with_context(|| {
                format!("Impossible de remplacer les opérations en attente du compte {asset_id}.")
            })?;
        for t in pending {
            sqlx::query(
                "INSERT INTO pending_transactions (asset_id, booked_on, amount, currency, label) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(t.asset_id.as_str())
            .bind(t.booked_on)
            .bind(t.amount.amount)
            .bind(t.amount.currency.code())
            .bind(&t.label)
            .execute(&mut *db_transaction)
            .await
            .with_context(|| {
                format!("Impossible d'écrire une opération en attente du compte {asset_id}.")
            })?;
        }
        db_transaction
            .commit()
            .await
            .context("Impossible de valider les opérations en attente.")?;
        Ok(())
    }

    async fn read_pending(&self) -> anyhow::Result<Vec<PendingTransaction>> {
        sqlx::query_as::<_, PendingRow>(
            "SELECT asset_id, booked_on, amount, currency, label \
             FROM pending_transactions ORDER BY booked_on, id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les opérations en attente.")?
        .into_iter()
        .map(PendingTransaction::try_from)
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedAssets;
    use crate::persistence::testdb::database;
    use asset::{Asset, AssetKind, AssetProvider};
    use rust_decimal::Decimal;
    use shared::Currency;

    fn account() -> Asset {
        Asset {
            id: AssetId::generate(),
            name: "Compte courant".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        }
    }

    fn transaction(asset: &Asset, label: &str, amount: i64, day: u32) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: asset.id.clone(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            amount: Money::new(Decimal::new(amount, 2), Currency::Eur),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: None,
        }
    }

    #[tokio::test]
    async fn ecrit_relit_dans_l_ordre_puis_supprime_les_transactions_d_un_compte() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let transactions = PersistedTransactions::new(pool);
        let (a, b) = (account(), account());
        assets.write(&a).await.unwrap();
        assets.write(&b).await.unwrap();
        let late = transaction(&a, "Salaire", 200_000, 9);
        let early = transaction(&a, "Loyer", -70_000, 5);
        let other = transaction(&b, "Autre compte", -100, 5);
        transactions
            .write_many(&[late.clone(), early.clone(), other.clone()])
            .await
            .unwrap();

        // Triées par date, et seulement celles du compte demandé.
        assert_eq!(
            transactions.read_of_asset(&a.id).await.unwrap(),
            vec![early, late]
        );

        transactions.delete_of_asset(&a.id).await.unwrap();
        assert!(transactions.read_of_asset(&a.id).await.unwrap().is_empty());
        assert_eq!(
            transactions.read_of_asset(&b.id).await.unwrap(),
            vec![other]
        );
        db.drop_db().await;
    }

    fn pending(asset: &Asset, label: &str, amount: i64, day: u32) -> PendingTransaction {
        PendingTransaction {
            asset_id: asset.id.clone(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            amount: Money::new(Decimal::new(amount, 2), Currency::Eur),
            label: label.into(),
        }
    }

    #[tokio::test]
    async fn chaque_lecture_remplace_les_operations_en_attente_du_compte() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let transactions = PersistedTransactions::new(pool);
        let (a, b) = (account(), account());
        assets.write(&a).await.unwrap();
        assets.write(&b).await.unwrap();
        let other = pending(&b, "Autre compte", -100, 6);
        transactions
            .replace_pending(&b.id, std::slice::from_ref(&other))
            .await
            .unwrap();
        transactions
            .replace_pending(
                &a.id,
                &[
                    pending(&a, "CB Boulangerie", -450, 25),
                    pending(&a, "CB Essence", -6_000, 24),
                ],
            )
            .await
            .unwrap();
        assert_eq!(transactions.read_pending().await.unwrap().len(), 3);

        // La lecture suivante remplace celles du compte, sans toucher à l'autre compte.
        let kept = pending(&a, "CB Essence", -6_000, 24);
        transactions
            .replace_pending(&a.id, std::slice::from_ref(&kept))
            .await
            .unwrap();
        assert_eq!(
            transactions.read_pending().await.unwrap(),
            vec![other.clone(), kept]
        );

        // Supprimer un compte emporte ses opérations en attente (la base refuse une ligne sans compte).
        transactions.delete_of_asset(&a.id).await.unwrap();
        assert_eq!(transactions.read_pending().await.unwrap(), vec![other]);
        db.drop_db().await;
    }

    #[tokio::test]
    async fn une_ecriture_qui_echoue_n_enregistre_rien() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let transactions = PersistedTransactions::new(pool);
        let a = account();
        assets.write(&a).await.unwrap();
        let good = transaction(&a, "Loyer", -70_000, 5);
        // Un compte inconnu : la base refuse la deuxième ligne (clé étrangère).
        let orphan = transaction(&account(), "Orpheline", -100, 6);

        assert!(transactions.write_many(&[good, orphan]).await.is_err());
        assert!(transactions.read_of_asset(&a.id).await.unwrap().is_empty());
        db.drop_db().await;
    }
}
