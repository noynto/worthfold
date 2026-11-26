use std::sync::Arc;

use asset::{AssetKind, AssetProvider};
use budget::BudgetProvider;
use category::CategoryProvider;
use chrono::NaiveDate;
use recurrence::RecurrenceProvider;
use rust_decimal::Decimal;
use service::budget::recommended_cushion;
use service::forecast::{Forecast, forecast};
use shared::TransactionId;
use shared::{Currency, Money};
use transaction::{Transaction, TransactionProvider, TransactionSource};
use valuation::ValuationProvider;

use crate::UseCaseError;

pub struct ReadForecast {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
    pub budget_provider: Arc<dyn BudgetProvider>,
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Query {
    /// La date du jour, fournie par l'appelant.
    pub today: NaiveDate,
    pub currency: Currency,
}

pub struct Output {
    pub forecast: Forecast,
    /// Le matelas (fixé à la main, sinon évalué) : le niveau sous lequel le solde ne devrait pas passer.
    pub cushion: Option<Money>,
    /// Le nombre de comptes courants, pour dire d'où vient le solde de départ.
    pub accounts: usize,
    /// Le nombre de paiements prévus qui nourrissent le prévisionnel.
    pub payments: usize,
    /// Les opérations en attente des comptes courants : leur nombre et leur somme (déjà ajoutée au solde de départ).
    pub pending: (usize, Decimal),
}

impl ReadForecast {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let currency = query.currency;
        let assets = self.asset_provider.read_all().await?;
        let valuations = self.valuation_provider.read_all().await?;
        let transactions = self.transaction_provider.read_all().await?;
        let categories = self.category_provider.read_all().await?;
        let payments = self.recurrence_provider.read_all().await?;
        let exceptions = self.recurrence_provider.read_exceptions().await?;
        let plan = self.budget_provider.read_savings_plan().await?;

        let current: Vec<_> = assets
            .iter()
            .filter(|a| a.kind == AssetKind::BankAccount && a.currency == currency)
            .collect();
        let booked: Decimal = current
            .iter()
            .map(|a| {
                service::latest_valuation(a, &valuations, query.today)
                    .map_or(Decimal::ZERO, |v| v.value.amount)
            })
            .sum();
        // Le solde de la banque ne contient pas les opérations en attente : on les ajoute, pour partir de ce
        // qui sera réellement disponible. Elles servent aussi à reconnaître un paiement déjà en route
        // (un loyer en attente ne doit pas être compté une seconde fois comme « prévu »).
        let pending: Vec<_> = self
            .transaction_provider
            .read_pending()
            .await?
            .into_iter()
            .filter(|p| p.amount.currency == currency && current.iter().any(|a| a.id == p.asset_id))
            .collect();
        let pending_total: Decimal = pending.iter().map(|p| p.amount.amount).sum();
        let start = booked + pending_total;
        let mut with_pending = transactions.clone();
        with_pending.extend(pending.iter().map(|p| Transaction {
            id: TransactionId::generate(),
            asset_id: p.asset_id.clone(),
            booked_on: p.booked_on,
            amount: p.amount,
            label: p.label.clone(),
            source: TransactionSource::Bank,
            category_id: None,
        }));
        // Le patrimoine n'est pas prédit si des devises sont mélangées : seul le solde l'est.
        let net_worth = service::net_worth(&assets, &valuations, query.today, currency)
            .ok()
            .map(|w| Money::new(w.amount + pending_total, currency));
        let cushion = plan.and_then(|p| p.cushion_override).or_else(|| {
            recommended_cushion(&transactions, &categories, query.today, currency).map(|c| c.amount)
        });

        Ok(Output {
            forecast: forecast(
                &payments,
                &exceptions,
                &with_pending,
                &categories,
                Money::new(start, currency),
                net_worth,
                query.today,
            ),
            cushion,
            accounts: current.len(),
            payments: payments.len(),
            pending: (pending.len(), pending_total),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::Asset;
    use recurrence::{Amount, AmountStep, DayOfMonth, Direction, PlannedPayment, Schedule, Window};
    use shared::PlannedPaymentId;
    use testing::{
        InMemoryAssets, InMemoryBudget, InMemoryCategories, InMemoryRecurrence,
        InMemoryTransactions, InMemoryValuations,
    };
    use valuation::Valuation;

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    #[tokio::test]
    async fn part_du_solde_des_comptes_courants_et_applique_les_echeances() {
        let day = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        let account = |id: &str, kind| Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
        };
        let valuation = |id: &str, amount| Valuation {
            id: shared::ValuationId::generate(),
            asset_id: id.into(),
            value: eur(amount),
            valued_on: day(11, 1),
        };
        let rent = PlannedPayment {
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
        };
        let output = ReadForecast {
            asset_provider: Arc::new(InMemoryAssets::with([
                account("compte", AssetKind::BankAccount),
                account("livret", AssetKind::SavingsBook),
            ])),
            valuation_provider: Arc::new(InMemoryValuations::with([
                valuation("compte", 1_000),
                valuation("livret", 5_000),
            ])),
            transaction_provider: Arc::new(InMemoryTransactions::default()),
            category_provider: Arc::new(InMemoryCategories::default()),
            budget_provider: Arc::new(InMemoryBudget::default()),
            recurrence_provider: Arc::new(InMemoryRecurrence::with([rent])),
        }
        .handle(Query {
            today: day(11, 30),
            currency: Currency::Eur,
        })
        .await
        .unwrap();

        // Seul le compte courant compte ; décembre : 1000 − 700, et le patrimoine suit.
        assert_eq!(output.forecast.start_balance, eur(1_000));
        assert_eq!(output.accounts, 1);
        let december = &output.forecast.months[1];
        assert_eq!(december.balance.expected, Decimal::from(300));
        assert_eq!(december.net_worth.unwrap().expected, Decimal::from(5_300));
        assert_eq!(output.cushion, None);
    }
    #[tokio::test]
    async fn les_operations_en_attente_baissent_le_solde_de_depart_et_le_patrimoine() {
        use transaction::PendingTransaction;
        let day = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        let account = |id: &str, kind| Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
        };
        let transactions = Arc::new(InMemoryTransactions::default());
        transactions
            .replace_pending(
                &"compte".into(),
                &[PendingTransaction {
                    asset_id: "compte".into(),
                    booked_on: day(11, 29),
                    amount: eur(-60),
                    label: "CB Essence".into(),
                }],
            )
            .await
            .unwrap();
        // Une opération en attente d'un compte qui n'est pas courant ne compte pas.
        transactions
            .replace_pending(
                &"livret".into(),
                &[PendingTransaction {
                    asset_id: "livret".into(),
                    booked_on: day(11, 29),
                    amount: eur(-1_000),
                    label: "Autre".into(),
                }],
            )
            .await
            .unwrap();
        let output = ReadForecast {
            asset_provider: Arc::new(InMemoryAssets::with([
                account("compte", AssetKind::BankAccount),
                account("livret", AssetKind::SavingsBook),
            ])),
            valuation_provider: Arc::new(InMemoryValuations::with([
                Valuation {
                    id: shared::ValuationId::generate(),
                    asset_id: "compte".into(),
                    value: eur(1_000),
                    valued_on: day(11, 1),
                },
                Valuation {
                    id: shared::ValuationId::generate(),
                    asset_id: "livret".into(),
                    value: eur(5_000),
                    valued_on: day(11, 1),
                },
            ])),
            transaction_provider: transactions,
            category_provider: Arc::new(InMemoryCategories::default()),
            budget_provider: Arc::new(InMemoryBudget::default()),
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
        }
        .handle(Query {
            today: day(11, 30),
            currency: Currency::Eur,
        })
        .await
        .unwrap();

        assert_eq!(output.forecast.start_balance, eur(940));
        assert_eq!(output.pending, (1, Decimal::from(-60)));
        // Le patrimoine suit : 940 + 5 000.
        assert_eq!(
            output.forecast.months[0].net_worth.unwrap().expected,
            Decimal::from(5_940)
        );
    }
}
