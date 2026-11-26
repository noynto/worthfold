use anyhow::Context;
use async_trait::async_trait;
use budget::{BudgetProvider, Envelope, SavingsPlan};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{AssetId, CategoryId, EnvelopeId, Money};
use sqlx::{FromRow, PgPool};

pub struct PersistedBudget {
    pool: PgPool,
}

impl PersistedBudget {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Un montant et sa devise, stockés dans deux colonnes (absentes ensemble, ou présentes ensemble).
fn money(amount: Option<Decimal>, currency: Option<String>) -> anyhow::Result<Option<Money>> {
    match (amount, currency) {
        (Some(amount), Some(currency)) => Ok(Some(Money::new(amount, currency.parse()?))),
        (None, None) => Ok(None),
        _ => anyhow::bail!("Montant et devise doivent être renseignés ensemble."),
    }
}

#[derive(FromRow)]
struct EnvelopeRow {
    id: String,
    category_id: String,
    amount: Option<Decimal>,
    currency: Option<String>,
    from_month: NaiveDate,
}

impl TryFrom<EnvelopeRow> for Envelope {
    type Error = anyhow::Error;

    fn try_from(row: EnvelopeRow) -> anyhow::Result<Self> {
        Ok(Envelope {
            id: EnvelopeId::from(row.id),
            category_id: CategoryId::from(row.category_id),
            limit: money(row.amount, row.currency)?,
            from_month: row.from_month,
        })
    }
}

#[derive(FromRow)]
struct PlanRow {
    destination: String,
    ceiling_amount: Decimal,
    ceiling_currency: String,
    cushion_amount: Option<Decimal>,
    cushion_currency: Option<String>,
}

impl TryFrom<PlanRow> for SavingsPlan {
    type Error = anyhow::Error;

    fn try_from(row: PlanRow) -> anyhow::Result<Self> {
        Ok(SavingsPlan {
            destination: AssetId::from(row.destination),
            ceiling: Money::new(row.ceiling_amount, row.ceiling_currency.parse()?),
            cushion_override: money(row.cushion_amount, row.cushion_currency)?,
        })
    }
}

#[async_trait]
impl BudgetProvider for PersistedBudget {
    /// Par ordre de saisie (les identifiants UUID v7 sont triés dans le temps).
    async fn read_envelopes(&self) -> anyhow::Result<Vec<Envelope>> {
        sqlx::query_as::<_, EnvelopeRow>(
            "SELECT id, category_id, amount, currency, from_month FROM envelopes ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les enveloppes.")?
        .into_iter()
        .map(Envelope::try_from)
        .collect()
    }

    async fn write_envelope(&self, envelope: &Envelope) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO envelopes (id, category_id, amount, currency, from_month) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(envelope.id.as_str())
        .bind(envelope.category_id.as_str())
        .bind(envelope.limit.map(|l| l.amount))
        .bind(envelope.limit.map(|l| l.currency.code()))
        .bind(envelope.from_month)
        .execute(&self.pool)
        .await
        .with_context(|| format!("Impossible d'écrire l'enveloppe {}.", envelope.id))?;
        Ok(())
    }

    async fn read_savings_plan(&self) -> anyhow::Result<Option<SavingsPlan>> {
        sqlx::query_as::<_, PlanRow>(
            "SELECT destination, ceiling_amount, ceiling_currency, cushion_amount, cushion_currency \
             FROM savings_plan WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("Impossible de lire le réglage de l'épargne.")?
        .map(SavingsPlan::try_from)
        .transpose()
    }

    async fn write_savings_plan(&self, plan: &SavingsPlan) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO savings_plan \
             (id, destination, ceiling_amount, ceiling_currency, cushion_amount, cushion_currency) \
             VALUES (1, $1, $2, $3, $4, $5) \
             ON CONFLICT (id) DO UPDATE SET destination = excluded.destination, \
             ceiling_amount = excluded.ceiling_amount, ceiling_currency = excluded.ceiling_currency, \
             cushion_amount = excluded.cushion_amount, cushion_currency = excluded.cushion_currency",
        )
        .bind(plan.destination.as_str())
        .bind(plan.ceiling.amount)
        .bind(plan.ceiling.currency.code())
        .bind(plan.cushion_override.map(|c| c.amount))
        .bind(plan.cushion_override.map(|c| c.currency.code()))
        .execute(&self.pool)
        .await
        .context("Impossible d'enregistrer le réglage de l'épargne.")?;
        Ok(())
    }

    async fn delete_envelopes_of_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM envelopes WHERE category_id = $1")
            .bind(category_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de supprimer les enveloppes de la catégorie {category_id}.")
            })?;
        Ok(())
    }

    async fn delete_savings_plan(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM savings_plan WHERE id = 1")
            .execute(&self.pool)
            .await
            .context("Impossible de supprimer le réglage de l'épargne.")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::testdb::database;
    use crate::persistence::{PersistedAssets, PersistedCategories};
    use asset::{Asset, AssetKind, AssetProvider};
    use category::{Category, CategoryKind, CategoryProvider};
    use rust_decimal::Decimal;
    use shared::Currency;

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    #[tokio::test]
    async fn ecrit_et_relit_les_enveloppes_dans_l_ordre_de_saisie_puis_les_supprime() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let categories = PersistedCategories::new(pool.clone());
        let budget = PersistedBudget::new(pool);
        let courses = Category {
            id: CategoryId::generate(),
            name: "Courses".into(),
            kind: CategoryKind::Expense,
        };
        categories.write(&courses).await.unwrap();
        let envelope = |limit: Option<Money>, month: u32| Envelope {
            id: EnvelopeId::generate(),
            category_id: courses.id.clone(),
            limit,
            from_month: NaiveDate::from_ymd_opt(2026, month, 1).unwrap(),
        };
        let (first, ended) = (envelope(Some(eur(400)), 1), envelope(None, 9));
        budget.write_envelope(&first).await.unwrap();
        budget.write_envelope(&ended).await.unwrap();

        assert_eq!(budget.read_envelopes().await.unwrap(), vec![first, ended]);

        budget
            .delete_envelopes_of_category(&courses.id)
            .await
            .unwrap();
        assert!(budget.read_envelopes().await.unwrap().is_empty());
        db.drop_db().await;
    }

    #[tokio::test]
    async fn le_reglage_de_l_epargne_se_remplace_et_se_supprime() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let budget = PersistedBudget::new(pool);
        let book = Asset {
            id: AssetId::generate(),
            name: "Livret A".into(),
            kind: AssetKind::SavingsBook,
            currency: Currency::Eur,
        };
        assets.write(&book).await.unwrap();
        assert_eq!(budget.read_savings_plan().await.unwrap(), None);

        let plan = SavingsPlan {
            destination: book.id.clone(),
            ceiling: eur(22_950),
            cushion_override: None,
        };
        budget.write_savings_plan(&plan).await.unwrap();
        assert_eq!(
            budget.read_savings_plan().await.unwrap(),
            Some(plan.clone())
        );

        // Le réglage remplace le précédent : toujours une seule ligne.
        let fixed = SavingsPlan {
            cushion_override: Some(eur(1_500)),
            ..plan
        };
        budget.write_savings_plan(&fixed).await.unwrap();
        assert_eq!(budget.read_savings_plan().await.unwrap(), Some(fixed));

        budget.delete_savings_plan().await.unwrap();
        assert_eq!(budget.read_savings_plan().await.unwrap(), None);
        db.drop_db().await;
    }
}
