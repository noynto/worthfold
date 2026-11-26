use anyhow::Context;
use async_trait::async_trait;
use category::{Category, CategoryKind, CategoryProvider, CategoryRule};
use shared::{CategoryId, CategoryRuleId};
use sqlx::{FromRow, PgPool};

pub struct PersistedCategories {
    pool: PgPool,
}

impl PersistedCategories {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct CategoryRow {
    id: String,
    name: String,
    kind: String,
}

impl TryFrom<CategoryRow> for Category {
    type Error = anyhow::Error;

    fn try_from(row: CategoryRow) -> anyhow::Result<Self> {
        Ok(Category {
            id: CategoryId::from(row.id),
            name: row.name,
            kind: CategoryKind::from_code(&row.kind)
                .with_context(|| format!("Sens de catégorie inconnu : {}.", row.kind))?,
        })
    }
}

#[derive(FromRow)]
struct RuleRow {
    id: String,
    category_id: String,
    pattern: String,
}

impl From<RuleRow> for CategoryRule {
    fn from(row: RuleRow) -> Self {
        CategoryRule {
            id: CategoryRuleId::from(row.id),
            category_id: CategoryId::from(row.category_id),
            pattern: row.pattern,
        }
    }
}

#[async_trait]
impl CategoryProvider for PersistedCategories {
    async fn read_all(&self) -> anyhow::Result<Vec<Category>> {
        sqlx::query_as::<_, CategoryRow>(
            "SELECT id, name, kind FROM categories ORDER BY lower(name), id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les catégories.")?
        .into_iter()
        .map(Category::try_from)
        .collect()
    }

    async fn read(&self, id: &CategoryId) -> anyhow::Result<Option<Category>> {
        sqlx::query_as::<_, CategoryRow>("SELECT id, name, kind FROM categories WHERE id = $1")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .with_context(|| format!("Impossible de lire la catégorie {id}."))?
            .map(Category::try_from)
            .transpose()
    }

    async fn write(&self, category: &Category) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO categories (id, name, kind) VALUES ($1, $2, $3)")
            .bind(category.id.as_str())
            .bind(&category.name)
            .bind(category.kind.code())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible d'écrire la catégorie {}.", category.id))?;
        Ok(())
    }

    async fn delete(&self, id: &CategoryId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM categories WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer la catégorie {id}."))?;
        Ok(())
    }

    async fn read_rules(&self) -> anyhow::Result<Vec<CategoryRule>> {
        Ok(sqlx::query_as::<_, RuleRow>(
            "SELECT id, category_id, pattern FROM category_rules ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les règles de catégories.")?
        .into_iter()
        .map(CategoryRule::from)
        .collect())
    }

    async fn read_rule(&self, id: &CategoryRuleId) -> anyhow::Result<Option<CategoryRule>> {
        Ok(sqlx::query_as::<_, RuleRow>(
            "SELECT id, category_id, pattern FROM category_rules WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Impossible de lire la règle {id}."))?
        .map(CategoryRule::from))
    }

    async fn write_rule(&self, rule: &CategoryRule) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO category_rules (id, category_id, pattern) VALUES ($1, $2, $3)")
            .bind(rule.id.as_str())
            .bind(rule.category_id.as_str())
            .bind(&rule.pattern)
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible d'écrire la règle {}.", rule.id))?;
        Ok(())
    }

    async fn delete_rule(&self, id: &CategoryRuleId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM category_rules WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer la règle {id}."))?;
        Ok(())
    }

    async fn delete_rules_of_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM category_rules WHERE category_id = $1")
            .bind(category_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de supprimer les règles de la catégorie {category_id}.")
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::testdb::database;
    use crate::persistence::{PersistedAssets, PersistedTransactions};
    use asset::{Asset, AssetKind, AssetProvider};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{AssetId, Currency, Money, TransactionId};
    use transaction::{Transaction, TransactionProvider, TransactionSource};

    fn category(name: &str, kind: CategoryKind) -> Category {
        Category {
            id: CategoryId::generate(),
            name: name.into(),
            kind,
        }
    }

    #[tokio::test]
    async fn ecrit_relit_et_supprime_categories_et_regles() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let categories = PersistedCategories::new(pool);
        let salary = category("Salaire", CategoryKind::Income);
        let groceries = category("courses", CategoryKind::Expense);
        categories.write(&salary).await.unwrap();
        categories.write(&groceries).await.unwrap();
        let rule = CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: groceries.id.clone(),
            pattern: "carrefour".into(),
        };
        categories.write_rule(&rule).await.unwrap();

        // Triées par nom sans tenir compte de la casse.
        assert_eq!(
            categories.read_all().await.unwrap(),
            vec![groceries.clone(), salary.clone()]
        );
        assert_eq!(categories.read(&salary.id).await.unwrap(), Some(salary));
        assert_eq!(categories.read_rules().await.unwrap(), vec![rule.clone()]);
        assert_eq!(
            categories.read_rule(&rule.id).await.unwrap(),
            Some(rule.clone())
        );

        categories
            .delete_rules_of_category(&groceries.id)
            .await
            .unwrap();
        assert!(categories.read_rules().await.unwrap().is_empty());
        categories.delete(&groceries.id).await.unwrap();
        assert_eq!(categories.read(&groceries.id).await.unwrap(), None);
        db.drop_db().await;
    }

    #[tokio::test]
    async fn classe_puis_declasse_les_transactions() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let categories = PersistedCategories::new(pool.clone());
        let transactions = PersistedTransactions::new(pool);
        let account = Asset {
            id: AssetId::generate(),
            name: "Compte".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        assets.write(&account).await.unwrap();
        let rent = category("Logement", CategoryKind::Expense);
        categories.write(&rent).await.unwrap();
        let transaction = |label: &str| Transaction {
            id: TransactionId::generate(),
            asset_id: account.id.clone(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            amount: Money::new(Decimal::from(-1), Currency::Eur),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: None,
        };
        let (a, b) = (transaction("a"), transaction("b"));
        transactions
            .write_many(&[a.clone(), b.clone()])
            .await
            .unwrap();

        transactions
            .set_category(&a.id, Some(&rent.id))
            .await
            .unwrap();
        transactions
            .set_categories(&[(b.id.clone(), rent.id.clone())])
            .await
            .unwrap();
        let read = transactions.read_all().await.unwrap();
        assert!(
            read.iter()
                .all(|t| t.category_id.as_ref() == Some(&rent.id))
        );
        assert_eq!(
            transactions.read(&a.id).await.unwrap().unwrap().category_id,
            Some(rent.id.clone())
        );

        transactions.set_category(&a.id, None).await.unwrap();
        assert_eq!(
            transactions.read(&a.id).await.unwrap().unwrap().category_id,
            None
        );

        transactions.clear_category(&rent.id).await.unwrap();
        assert!(
            transactions
                .read_all()
                .await
                .unwrap()
                .iter()
                .all(|t| t.category_id.is_none())
        );
        db.drop_db().await;
    }
}
