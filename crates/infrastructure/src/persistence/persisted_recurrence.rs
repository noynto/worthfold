use anyhow::{Context, bail};
use async_trait::async_trait;
use chrono::NaiveDate;
use recurrence::{
    Amount, AmountStep, DayOfMonth, Direction, ExceptionAction, PlannedPayment, RecurrenceProvider,
    Revision, Schedule, Window,
};
use rust_decimal::Decimal;
use shared::{AssetId, CategoryId, Money, PlannedPaymentId};
use sqlx::{FromRow, PgPool};

pub struct PersistedRecurrence {
    pool: PgPool,
}

impl PersistedRecurrence {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct PaymentRow {
    id: String,
    label: String,
    pattern: String,
    direction: String,
    account_id: String,
    category_id: Option<String>,
    schedule_kind: String,
    day_kind: Option<String>,
    day_value: Option<i32>,
    every_n: Option<i32>,
    first_month: Option<NaiveDate>,
    months: Option<String>,
    once_date: Option<NaiveDate>,
    observed: bool,
    revision_month: Option<i32>,
    revision_day: Option<i32>,
    revision_change: Option<Decimal>,
    window_from: Option<NaiveDate>,
    window_until: Option<NaiveDate>,
    window_count: Option<i32>,
}

#[derive(FromRow)]
struct StepRow {
    payment_id: String,
    from_date: NaiveDate,
    amount: Decimal,
    margin: Decimal,
    currency: String,
}

fn day_of_month(kind: Option<&str>, value: Option<i32>) -> anyhow::Result<DayOfMonth> {
    match (kind, value) {
        (Some("day"), Some(d)) => Ok(DayOfMonth::Day(u32::try_from(d)?)),
        (Some("last"), _) => Ok(DayOfMonth::Last),
        _ => bail!("Jour du mois invalide en base."),
    }
}

impl PaymentRow {
    fn into_payment(self, steps: Vec<AmountStep>) -> anyhow::Result<PlannedPayment> {
        let schedule = match self.schedule_kind.as_str() {
            "monthly" => Schedule::Monthly {
                day: day_of_month(self.day_kind.as_deref(), self.day_value)?,
            },
            "every_n_months" => Schedule::EveryNMonths {
                n: u32::try_from(self.every_n.context("Rythme sans nombre de mois.")?)?,
                day: day_of_month(self.day_kind.as_deref(), self.day_value)?,
                first_month: self.first_month.context("Rythme sans premier mois.")?,
            },
            "on_months" => Schedule::OnMonths {
                months: self
                    .months
                    .as_deref()
                    .context("Rythme sans mois.")?
                    .split(',')
                    .map(|m| m.trim().parse())
                    .collect::<Result<_, _>>()?,
                day: day_of_month(self.day_kind.as_deref(), self.day_value)?,
            },
            "once" => Schedule::Once {
                date: self.once_date.context("Échéance unique sans date.")?,
            },
            other => bail!("Rythme inconnu en base : {other}."),
        };
        let revision = match (self.revision_month, self.revision_day, self.revision_change) {
            (Some(month), Some(day), Some(change)) => Some(Revision {
                month: u32::try_from(month)?,
                day: u32::try_from(day)?,
                change,
            }),
            (None, None, None) => None,
            _ => bail!("Révision incomplète en base."),
        };
        Ok(PlannedPayment {
            id: PlannedPaymentId::from(self.id),
            label: self.label,
            pattern: self.pattern,
            direction: Direction::from_code(&self.direction)
                .with_context(|| format!("Sens inconnu en base : {}.", self.direction))?,
            account_id: AssetId::from(self.account_id),
            category_id: self.category_id.map(CategoryId::from),
            schedule,
            amount: Amount {
                steps,
                revision,
                observed: self.observed,
            },
            window: Window {
                from: self.window_from,
                until: self.window_until,
                occurrences: self.window_count.map(u32::try_from).transpose()?,
            },
        })
    }
}

impl PersistedRecurrence {
    async fn steps_of(&self, payment_id: Option<&str>) -> anyhow::Result<Vec<StepRow>> {
        let rows = match payment_id {
            Some(id) => {
                sqlx::query_as::<_, StepRow>(
                    "SELECT payment_id, from_date, amount, margin, currency \
                     FROM planned_payment_steps WHERE payment_id = $1 ORDER BY from_date",
                )
                .bind(id)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, StepRow>(
                    "SELECT payment_id, from_date, amount, margin, currency \
                     FROM planned_payment_steps ORDER BY payment_id, from_date",
                )
                .fetch_all(&self.pool)
                .await
            }
        };
        rows.context("Impossible de lire les paliers de montant.")
    }
}

fn steps_from(rows: Vec<StepRow>) -> anyhow::Result<Vec<AmountStep>> {
    rows.into_iter()
        .map(|r| {
            let currency = r.currency.parse()?;
            Ok(AmountStep {
                from: r.from_date,
                value: Money::new(r.amount, currency),
                margin: Money::new(r.margin, currency),
            })
        })
        .collect()
}

/// Les colonnes d'un paiement, partagées par les requêtes (assemblées à la compilation, jamais à l'exécution).
macro_rules! payment_columns {
    () => {
        "id, label, pattern, direction, account_id, category_id, schedule_kind, day_kind, day_value, \
         every_n, first_month, months, once_date, observed, revision_month, revision_day, \
         revision_change, window_from, window_until, window_count"
    };
}

#[async_trait]
impl RecurrenceProvider for PersistedRecurrence {
    /// Par ordre de création (les identifiants UUID v7 sont triés dans le temps).
    async fn read_all(&self) -> anyhow::Result<Vec<PlannedPayment>> {
        let rows = sqlx::query_as::<_, PaymentRow>(concat!(
            "SELECT ",
            payment_columns!(),
            " FROM planned_payments ORDER BY id"
        ))
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les paiements prévus.")?;
        let mut steps = self.steps_of(None).await?;
        rows.into_iter()
            .map(|row| {
                let (own, rest): (Vec<_>, Vec<_>) =
                    steps.drain(..).partition(|s| s.payment_id == row.id);
                steps = rest;
                row.into_payment(steps_from(own)?)
            })
            .collect()
    }

    async fn read(&self, id: &PlannedPaymentId) -> anyhow::Result<Option<PlannedPayment>> {
        let Some(row) = sqlx::query_as::<_, PaymentRow>(concat!(
            "SELECT ",
            payment_columns!(),
            " FROM planned_payments WHERE id = $1"
        ))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Impossible de lire le paiement {id}."))?
        else {
            return Ok(None);
        };
        let steps = steps_from(self.steps_of(Some(id.as_str())).await?)?;
        row.into_payment(steps).map(Some)
    }

    async fn write(&self, payment: &PlannedPayment) -> anyhow::Result<()> {
        let (kind, day, every_n, first_month, months, once_date) = match &payment.schedule {
            Schedule::Monthly { day } => ("monthly", Some(*day), None, None, None, None),
            Schedule::EveryNMonths {
                n,
                day,
                first_month,
            } => (
                "every_n_months",
                Some(*day),
                Some(*n),
                Some(*first_month),
                None,
                None,
            ),
            Schedule::OnMonths { months, day } => (
                "on_months",
                Some(*day),
                None,
                None,
                Some(
                    months
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                None,
            ),
            Schedule::Once { date } => ("once", None, None, None, None, Some(*date)),
        };
        let (day_kind, day_value) = match day {
            Some(DayOfMonth::Day(d)) => (Some("day"), Some(i32::try_from(d)?)),
            Some(DayOfMonth::Last) => (Some("last"), None),
            None => (None, None),
        };
        let revision = payment.amount.revision;

        let mut tx = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction de base de données.")?;
        sqlx::query(
            "INSERT INTO planned_payments (id, label, pattern, direction, account_id, category_id, \
             schedule_kind, day_kind, day_value, every_n, first_month, months, once_date, observed, \
             revision_month, revision_day, revision_change, window_from, window_until, window_count) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20) \
             ON CONFLICT(id) DO UPDATE SET label = excluded.label, pattern = excluded.pattern, \
             direction = excluded.direction, account_id = excluded.account_id, \
             category_id = excluded.category_id, schedule_kind = excluded.schedule_kind, \
             day_kind = excluded.day_kind, day_value = excluded.day_value, every_n = excluded.every_n, \
             first_month = excluded.first_month, months = excluded.months, once_date = excluded.once_date, \
             observed = excluded.observed, revision_month = excluded.revision_month, \
             revision_day = excluded.revision_day, revision_change = excluded.revision_change, \
             window_from = excluded.window_from, window_until = excluded.window_until, \
             window_count = excluded.window_count",
        )
        .bind(payment.id.as_str())
        .bind(&payment.label)
        .bind(&payment.pattern)
        .bind(payment.direction.code())
        .bind(payment.account_id.as_str())
        .bind(payment.category_id.as_ref().map(CategoryId::as_str))
        .bind(kind)
        .bind(day_kind)
        .bind(day_value)
        .bind(every_n.map(i32::try_from).transpose()?)
        .bind(first_month)
        .bind(months)
        .bind(once_date)
        .bind(payment.amount.observed)
        .bind(revision.map(|r| i32::try_from(r.month)).transpose()?)
        .bind(revision.map(|r| i32::try_from(r.day)).transpose()?)
        .bind(revision.map(|r| r.change))
        .bind(payment.window.from)
        .bind(payment.window.until)
        .bind(payment.window.occurrences.map(i32::try_from).transpose()?)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("Impossible d'écrire le paiement {}.", payment.id))?;

        // Les paliers sont réécrits avec le paiement : le cas d'usage a déjà ajouté le nouveau à la liste.
        sqlx::query("DELETE FROM planned_payment_steps WHERE payment_id = $1")
            .bind(payment.id.as_str())
            .execute(&mut *tx)
            .await
            .context("Impossible de remplacer les paliers de montant.")?;
        for step in &payment.amount.steps {
            sqlx::query(
                "INSERT INTO planned_payment_steps (payment_id, from_date, amount, margin, currency) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(payment.id.as_str())
            .bind(step.from)
            .bind(step.value.amount)
            .bind(step.margin.amount)
            .bind(step.value.currency.code())
            .execute(&mut *tx)
            .await
            .context("Impossible d'écrire un palier de montant.")?;
        }
        tx.commit()
            .await
            .context("Impossible de valider l'écriture du paiement.")?;
        Ok(())
    }

    async fn delete(&self, id: &PlannedPaymentId) -> anyhow::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction.")?;
        sqlx::query("DELETE FROM planned_payment_exceptions WHERE payment_id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .context("Impossible de supprimer les exceptions du paiement.")?;
        sqlx::query("DELETE FROM planned_payment_steps WHERE payment_id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .context("Impossible de supprimer les paliers du paiement.")?;
        sqlx::query("DELETE FROM planned_payments WHERE id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Impossible de supprimer le paiement {id}."))?;
        tx.commit()
            .await
            .context("Impossible de valider la suppression.")?;
        Ok(())
    }

    async fn read_exceptions(
        &self,
    ) -> anyhow::Result<Vec<(PlannedPaymentId, NaiveDate, ExceptionAction)>> {
        #[derive(FromRow)]
        struct Row {
            payment_id: String,
            original_date: NaiveDate,
            action: String,
            amount: Option<Decimal>,
            currency: Option<String>,
        }
        sqlx::query_as::<_, Row>(
            "SELECT payment_id, original_date, action, amount, currency \
             FROM planned_payment_exceptions ORDER BY payment_id, original_date",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les exceptions.")?
        .into_iter()
        .map(|r| {
            let action = match (r.action.as_str(), r.amount, r.currency) {
                ("skip", _, _) => ExceptionAction::Skip,
                ("amount", Some(amount), Some(currency)) => {
                    ExceptionAction::Amount(Money::new(amount, currency.parse()?))
                }
                (other, _, _) => bail!("Exception invalide en base : {other}."),
            };
            Ok((
                PlannedPaymentId::from(r.payment_id),
                r.original_date,
                action,
            ))
        })
        .collect()
    }

    async fn write_exception(
        &self,
        payment_id: &PlannedPaymentId,
        on: NaiveDate,
        action: ExceptionAction,
    ) -> anyhow::Result<()> {
        let (kind, amount) = match action {
            ExceptionAction::Skip => ("skip", None),
            ExceptionAction::Amount(m) => ("amount", Some(m)),
        };
        sqlx::query(
            "INSERT INTO planned_payment_exceptions (payment_id, original_date, action, amount, currency) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT(payment_id, original_date) DO UPDATE SET action = excluded.action, \
             amount = excluded.amount, currency = excluded.currency",
        )
        .bind(payment_id.as_str())
        .bind(on)
        .bind(kind)
        .bind(amount.map(|m| m.amount))
        .bind(amount.map(|m| m.currency.code()))
        .execute(&self.pool)
        .await
        .with_context(|| format!("Impossible d'écrire l'exception du paiement {payment_id}."))?;
        Ok(())
    }

    async fn delete_exception(
        &self,
        payment_id: &PlannedPaymentId,
        on: NaiveDate,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM planned_payment_exceptions WHERE payment_id = $1 AND original_date = $2",
        )
        .bind(payment_id.as_str())
        .bind(on)
        .execute(&self.pool)
        .await
        .with_context(|| {
            format!("Impossible de supprimer l'exception du paiement {payment_id}.")
        })?;
        Ok(())
    }

    async fn clear_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        sqlx::query("UPDATE planned_payments SET category_id = NULL WHERE category_id = $1")
            .bind(category_id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| {
                format!("Impossible de détacher les paiements de la catégorie {category_id}.")
            })?;
        Ok(())
    }

    async fn delete_of_account(&self, account_id: &AssetId) -> anyhow::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction.")?;
        sqlx::query(
            "DELETE FROM planned_payment_exceptions \
             WHERE payment_id IN (SELECT id FROM planned_payments WHERE account_id = $1)",
        )
        .bind(account_id.as_str())
        .execute(&mut *tx)
        .await
        .context("Impossible de supprimer les exceptions du compte.")?;
        sqlx::query(
            "DELETE FROM planned_payment_steps \
             WHERE payment_id IN (SELECT id FROM planned_payments WHERE account_id = $1)",
        )
        .bind(account_id.as_str())
        .execute(&mut *tx)
        .await
        .context("Impossible de supprimer les paliers du compte.")?;
        sqlx::query("DELETE FROM planned_payments WHERE account_id = $1")
            .bind(account_id.as_str())
            .execute(&mut *tx)
            .await
            .with_context(|| {
                format!("Impossible de supprimer les paiements du compte {account_id}.")
            })?;
        tx.commit()
            .await
            .context("Impossible de valider la suppression.")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedAssets;
    use crate::persistence::testdb::{TestDb, database};
    use asset::{Asset, AssetKind, AssetProvider};
    use rust_decimal::Decimal;
    use shared::Currency;

    async fn setup() -> Option<(PersistedRecurrence, Asset, TestDb)> {
        let db = database().await?;
        let pool = db.pool.clone();
        let account = Asset {
            id: AssetId::generate(),
            name: "Compte".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
        };
        PersistedAssets::new(pool.clone())
            .write(&account)
            .await
            .unwrap();
        Some((PersistedRecurrence::new(pool), account, db))
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn payment(account: &Asset, schedule: Schedule) -> PlannedPayment {
        PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: "Fermage".into(),
            pattern: "fermage".into(),
            direction: Direction::In,
            account_id: account.id.clone(),
            category_id: None,
            schedule,
            amount: Amount {
                steps: vec![
                    AmountStep {
                        from: day(2026, 1, 1),
                        value: eur(4000),
                        margin: eur(0),
                    },
                    AmountStep {
                        from: day(2026, 10, 15),
                        value: eur(4080),
                        margin: eur(50),
                    },
                ],
                revision: Some(Revision {
                    month: 10,
                    day: 15,
                    change: Decimal::new(25, 1),
                }),
                observed: true,
            },
            window: Window {
                from: Some(day(2026, 1, 1)),
                until: Some(day(2030, 12, 31)),
                occurrences: Some(20),
            },
        }
    }

    #[tokio::test]
    async fn chaque_rythme_fait_l_aller_retour_avec_ses_paliers_et_sa_fenetre() {
        let Some((provider, account, db)) = setup().await else {
            return;
        };
        let schedules = [
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            Schedule::EveryNMonths {
                n: 3,
                day: DayOfMonth::Last,
                first_month: day(2026, 2, 1),
            },
            Schedule::OnMonths {
                months: vec![10, 11, 12],
                day: DayOfMonth::Day(15),
            },
            Schedule::Once {
                date: day(2026, 12, 20),
            },
        ];
        let written: Vec<_> = schedules
            .into_iter()
            .map(|s| payment(&account, s))
            .collect();
        for p in &written {
            provider.write(p).await.unwrap();
        }
        assert_eq!(provider.read_all().await.unwrap(), written);
        assert_eq!(
            provider.read(&written[1].id).await.unwrap(),
            Some(written[1].clone())
        );
        assert_eq!(
            provider.read(&PlannedPaymentId::generate()).await.unwrap(),
            None
        );
        db.drop_db().await;
    }

    #[tokio::test]
    async fn reecrire_un_paiement_remplace_ses_paliers_sans_le_dupliquer() {
        let Some((provider, account, db)) = setup().await else {
            return;
        };
        let mut p = payment(
            &account,
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
        );
        provider.write(&p).await.unwrap();
        p.amount.steps.push(AmountStep {
            from: day(2027, 1, 1),
            value: eur(4200),
            margin: eur(0),
        });
        p.amount.revision = None;
        p.window = Window::default();
        provider.write(&p).await.unwrap();

        let all = provider.read_all().await.unwrap();
        assert_eq!(all, vec![p]);
        assert_eq!(all[0].amount.steps.len(), 3);
        db.drop_db().await;
    }

    #[tokio::test]
    async fn les_exceptions_se_posent_se_remplacent_et_se_retirent() {
        let Some((provider, account, db)) = setup().await else {
            return;
        };
        let p = payment(
            &account,
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
        );
        provider.write(&p).await.unwrap();
        provider
            .write_exception(&p.id, day(2026, 11, 5), ExceptionAction::Skip)
            .await
            .unwrap();
        provider
            .write_exception(&p.id, day(2026, 12, 5), ExceptionAction::Amount(eur(250)))
            .await
            .unwrap();
        // Une seconde exception sur la même échéance remplace la première.
        provider
            .write_exception(&p.id, day(2026, 11, 5), ExceptionAction::Amount(eur(10)))
            .await
            .unwrap();

        assert_eq!(
            provider.read_exceptions().await.unwrap(),
            vec![
                (
                    p.id.clone(),
                    day(2026, 11, 5),
                    ExceptionAction::Amount(eur(10))
                ),
                (
                    p.id.clone(),
                    day(2026, 12, 5),
                    ExceptionAction::Amount(eur(250))
                ),
            ]
        );
        provider
            .delete_exception(&p.id, day(2026, 11, 5))
            .await
            .unwrap();
        assert_eq!(provider.read_exceptions().await.unwrap().len(), 1);
        db.drop_db().await;
    }

    #[tokio::test]
    async fn supprimer_un_paiement_ou_un_compte_emporte_paliers_et_exceptions() {
        let Some((provider, account, db)) = setup().await else {
            return;
        };
        let (a, b) = (
            payment(
                &account,
                Schedule::Monthly {
                    day: DayOfMonth::Day(5),
                },
            ),
            payment(
                &account,
                Schedule::Once {
                    date: day(2026, 12, 20),
                },
            ),
        );
        provider.write(&a).await.unwrap();
        provider.write(&b).await.unwrap();
        provider
            .write_exception(&a.id, day(2026, 11, 5), ExceptionAction::Skip)
            .await
            .unwrap();

        provider.delete(&a.id).await.unwrap();
        assert_eq!(provider.read_all().await.unwrap(), vec![b.clone()]);
        assert!(provider.read_exceptions().await.unwrap().is_empty());

        provider
            .write_exception(&b.id, day(2026, 12, 20), ExceptionAction::Skip)
            .await
            .unwrap();
        provider.delete_of_account(&account.id).await.unwrap();
        assert!(provider.read_all().await.unwrap().is_empty());
        assert!(provider.read_exceptions().await.unwrap().is_empty());
        db.drop_db().await;
    }

    #[tokio::test]
    async fn detacher_une_categorie_garde_le_paiement() {
        let Some((provider, account, db)) = setup().await else {
            return;
        };
        let pool = provider.pool.clone();
        let category = category::Category {
            id: CategoryId::generate(),
            name: "Revenus".into(),
            kind: category::CategoryKind::Income,
        };
        category::CategoryProvider::write(
            &crate::persistence::PersistedCategories::new(pool),
            &category,
        )
        .await
        .unwrap();
        let mut p = payment(
            &account,
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
        );
        p.category_id = Some(category.id.clone());
        provider.write(&p).await.unwrap();

        provider.clear_category(&category.id).await.unwrap();
        let read = provider.read(&p.id).await.unwrap().unwrap();
        assert_eq!(read.category_id, None);
        db.drop_db().await;
    }
}
