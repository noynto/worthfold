use std::sync::Arc;

use asset::{Asset, AssetKind, AssetProvider};
use category::{Category, CategoryProvider};
use chrono::NaiveDate;
use recurrence::{PlannedPayment, RecurrenceProvider};
use service::recurrence::{CalendarLine, Suggestion, calendar, detect};
use shared::PlannedPaymentId;
use transaction::TransactionProvider;

use crate::UseCaseError;

pub struct ReadPlannedPayments {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Query {
    /// La date du jour, fournie par l'appelant.
    pub today: NaiveDate,
    /// Combien de jours en arrière regarder (échéances honorées ou manquantes).
    pub back_days: u64,
    /// Combien de jours en avant (échéances attendues).
    pub ahead_days: u64,
}

pub struct Output {
    /// Les paiements prévus, du plus ancien au plus récent.
    pub payments: Vec<PlannedPayment>,
    /// Les échéances autour d'aujourd'hui, par date, rapprochées des opérations réelles.
    pub calendar: Vec<CalendarLine>,
    /// Les échéances qui ont une exception (sautée ou changée) : paiement et date.
    pub exceptions: Vec<(PlannedPaymentId, NaiveDate)>,
    /// Ce que l'historique laisse penser régulier, sans que ce soit déjà déclaré.
    pub suggestions: Vec<Suggestion>,
    /// Les comptes bancaires, pour rattacher un paiement.
    pub accounts: Vec<Asset>,
    pub categories: Vec<Category>,
}

impl ReadPlannedPayments {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let payments = self.recurrence_provider.read_all().await?;
        let exceptions = self.recurrence_provider.read_exceptions().await?;
        let transactions = self.transaction_provider.read_all().await?;
        let categories = self.category_provider.read_all().await?;
        let accounts = self
            .asset_provider
            .read_all()
            .await?
            .into_iter()
            .filter(|a| a.kind == AssetKind::BankAccount)
            .collect();
        Ok(Output {
            calendar: calendar(
                &payments,
                &exceptions,
                &transactions,
                query.today,
                query.back_days,
                query.ahead_days,
            ),
            suggestions: detect(&transactions, &categories, &payments),
            exceptions: exceptions
                .iter()
                .map(|(id, date, _)| (id.clone(), *date))
                .collect(),
            payments,
            accounts,
            categories,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recurrence::{Amount, AmountStep, DayOfMonth, Direction, Schedule, Window};
    use rust_decimal::Decimal;
    use service::recurrence::OccurrenceStatus;
    use shared::{Currency, Money, PlannedPaymentId, TransactionId};
    use testing::{InMemoryAssets, InMemoryCategories, InMemoryRecurrence, InMemoryTransactions};
    use transaction::{Transaction, TransactionSource};

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn asset(id: &str, kind: AssetKind) -> Asset {
        Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
        }
    }

    fn transaction(date: NaiveDate, amount: i64, label: &str) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: "compte".into(),
            booked_on: date,
            amount: eur(amount),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: None,
        }
    }

    fn loyer() -> PlannedPayment {
        PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: "Loyer".into(),
            pattern: "loyer".into(),
            direction: Direction::Out,
            account_id: "compte".into(),
            category_id: None,
            schedule: Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            amount: Amount {
                steps: vec![AmountStep {
                    from: day(1, 1),
                    value: eur(700),
                    margin: eur(0),
                }],
                revision: None,
                observed: false,
            },
            window: Window::default(),
        }
    }

    #[tokio::test]
    async fn rend_le_calendrier_les_comptes_bancaires_et_les_suggestions() {
        let mut transactions = vec![transaction(day(10, 5), -700, "PRLV LOYER")];
        transactions.extend((8..=11).map(|m| transaction(day(m, 9), -12, "PRLV NETFLIX")));
        let output = ReadPlannedPayments {
            asset_provider: Arc::new(InMemoryAssets::with([
                asset("compte", AssetKind::BankAccount),
                asset("livret", AssetKind::SavingsBook),
            ])),
            category_provider: Arc::new(InMemoryCategories::default()),
            transaction_provider: Arc::new(InMemoryTransactions::with(transactions)),
            recurrence_provider: Arc::new(InMemoryRecurrence::with([loyer()])),
        }
        .handle(Query {
            today: day(11, 20),
            back_days: 60,
            ahead_days: 45,
        })
        .await
        .unwrap();

        // Seul le compte bancaire est proposé pour rattacher un paiement.
        assert_eq!(output.accounts.len(), 1);
        // Le loyer d'octobre est honoré, celui de novembre est manquant (le 5, aujourd'hui le 20),
        // celui de décembre est attendu.
        let statuses: Vec<_> = output
            .calendar
            .iter()
            .map(|l| {
                (
                    l.line.occurrence.date,
                    matches!(l.line.status, OccurrenceStatus::Honored { .. }),
                )
            })
            .collect();
        assert_eq!(
            statuses,
            [(day(10, 5), true), (day(11, 5), false), (day(12, 5), false)]
        );
        assert_eq!(output.calendar[1].line.status, OccurrenceStatus::Missing);
        assert_eq!(output.calendar[2].line.status, OccurrenceStatus::Expected);
        // Netflix revient chaque mois sans être déclaré : il est suggéré ; le loyer, déjà déclaré, non.
        let patterns: Vec<_> = output
            .suggestions
            .iter()
            .map(|s| s.pattern.as_str())
            .collect();
        assert_eq!(patterns, ["netflix"]);
    }
}
