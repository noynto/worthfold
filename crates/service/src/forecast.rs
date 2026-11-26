//! Le prévisionnel de trésorerie : le solde des comptes courants mois après mois, d'après les échéances
//! déclarées et les dépenses variables des derniers mois complets. Des fonctions pures : tout est lu
//! en amont, la date du jour est un paramètre.
//!
//! Trois scénarios, explicables en une phrase chacun :
//! - **prudent** : chaque échéance au pire de sa marge, et le mois variable le plus cher ;
//! - **attendu** : chaque échéance à sa valeur, et la moyenne des mois variables ;
//! - **favorable** : chaque échéance au mieux de sa marge, et le mois variable le moins cher.
//!
//! Les versements vers le livret ne sont pas prévus (c'est une suggestion, pas un engagement) ; ils ne
//! changent d'ailleurs pas le patrimoine net.

use budget::{month_start, next_month};
use category::{Category, CategoryKind};
use chrono::{Days, NaiveDate};
use recurrence::{ExceptionAction, PlannedPayment};
use rust_decimal::{Decimal, RoundingStrategy};
use shared::{CategoryId, Money};
use std::collections::HashMap;
use transaction::Transaction;

use crate::budget::complete_months;
use crate::recurrence::{OccurrenceStatus, matches, payment_lines};

/// Combien de mois entiers après le mois en cours le prévisionnel couvre.
pub const HORIZON_MONTHS: usize = 12;

/// Les dépenses qui ne sont pas des échéances déclarées, mesurées sur les derniers mois complets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariableSpending {
    /// La moyenne des mois : le scénario attendu.
    pub average: Decimal,
    /// Le mois le plus cher : le scénario prudent.
    pub highest: Decimal,
    /// Le mois le moins cher : le scénario favorable.
    pub lowest: Decimal,
    /// Combien de mois complets ont servi.
    pub months: usize,
}

/// Un montant dans les trois scénarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scenarios {
    pub low: Decimal,
    pub expected: Decimal,
    pub high: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastMonth {
    /// Le premier jour du mois.
    pub month: NaiveDate,
    /// Les échéances de revenu attendues ce mois-là (attendu).
    pub planned_income: Decimal,
    /// Les échéances de dépense attendues ce mois-là (positif, attendu).
    pub planned_expenses: Decimal,
    /// Les dépenses variables attendues ce mois-là (positif).
    pub variable_expenses: Decimal,
    /// Le solde des comptes courants à la fin du mois.
    pub balance: Scenarios,
    /// Le patrimoine net à la fin du mois, quand il est calculable : les autres actifs ne bougent pas.
    pub net_worth: Option<Scenarios>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forecast {
    pub start_balance: Money,
    /// Absent tant qu'aucun mois complet n'est connu : les dépenses variables ne sont alors pas prévues.
    pub variable: Option<VariableSpending>,
    /// Le mois en cours (reste du mois), puis `HORIZON_MONTHS` mois entiers.
    pub months: Vec<ForecastMonth>,
}

impl Forecast {
    /// Le premier mois où le solde passe sous `level` dans le scénario choisi.
    pub fn first_below(
        &self,
        level: Decimal,
        scenario: impl Fn(&Scenarios) -> Decimal,
    ) -> Option<&ForecastMonth> {
        self.months.iter().find(|m| scenario(&m.balance) < level)
    }
}

/// Les dépenses variables d'un mois : les sorties sans rapport avec un paiement déclaré, hors virements
/// internes et revenus. Un remboursement classé en dépense vient en déduction.
fn variable_in_month(
    transactions: &[Transaction],
    kinds: &HashMap<&CategoryId, CategoryKind>,
    payments: &[PlannedPayment],
    month: NaiveDate,
    currency: shared::Currency,
) -> Decimal {
    let spent: Decimal = transactions
        .iter()
        .filter(|t| t.amount.currency == currency && month_start(t.booked_on) == month)
        .filter(|t| !payments.iter().any(|p| matches(p, t)))
        .filter_map(|t| {
            let kind = t.category_id.as_ref().and_then(|id| kinds.get(id)).copied();
            match kind {
                Some(CategoryKind::Transfer | CategoryKind::Income) => None,
                Some(CategoryKind::Expense) => Some(-t.amount.amount),
                None => (t.amount.amount < Decimal::ZERO).then_some(-t.amount.amount),
            }
        })
        .sum();
    spent.max(Decimal::ZERO)
}

/// Mesure les dépenses variables sur les derniers mois complets. `None` sans mois complet.
pub fn variable_spending(
    transactions: &[Transaction],
    categories: &[Category],
    payments: &[PlannedPayment],
    today: NaiveDate,
    currency: shared::Currency,
) -> Option<VariableSpending> {
    let kinds: HashMap<&CategoryId, CategoryKind> =
        categories.iter().map(|c| (&c.id, c.kind)).collect();
    let totals: Vec<Decimal> = complete_months(transactions, today, currency)
        .into_iter()
        .map(|m| variable_in_month(transactions, &kinds, payments, m, currency))
        .collect();
    let (highest, lowest) = (totals.iter().max()?, totals.iter().min()?);
    let average = (totals.iter().sum::<Decimal>() / Decimal::from(totals.len()))
        .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    Some(VariableSpending {
        average,
        highest: *highest,
        lowest: *lowest,
        months: totals.len(),
    })
}

/// Le prévisionnel des comptes courants. `start_balance` : ce que contiennent les comptes courants
/// aujourd'hui ; `net_worth` : le patrimoine net d'aujourd'hui, s'il est calculable.
pub fn forecast(
    payments: &[PlannedPayment],
    exceptions: &[(shared::PlannedPaymentId, NaiveDate, ExceptionAction)],
    transactions: &[Transaction],
    categories: &[Category],
    start_balance: Money,
    net_worth: Option<Money>,
    today: NaiveDate,
) -> Forecast {
    let currency = start_balance.currency;
    let variable = variable_spending(transactions, categories, payments, today, currency);

    let first = month_start(today);
    let mut starts = vec![first];
    for _ in 0..HORIZON_MONTHS {
        starts.push(next_month(*starts.last().expect("au moins un mois")));
    }
    let end = next_month(*starts.last().expect("au moins un mois"))
        .pred_opt()
        .unwrap_or(today);

    // Les échéances encore à venir, y compris celles dont la date est passée depuis peu mais dont
    // l'opération n'est pas encore arrivée : elles ne sont pas dans le solde. (income, expenses, margin)
    let mut per_month: HashMap<NaiveDate, (Decimal, Decimal, Decimal)> = HashMap::new();
    let from = today - Days::new(crate::recurrence::MATCH_DAYS_AFTER);
    for payment in payments {
        for line in payment_lines(payment, exceptions, transactions, today, from, end) {
            let occurrence = &line.occurrence;
            if line.status != OccurrenceStatus::Expected || occurrence.amount.currency != currency {
                continue;
            }
            let bucket = per_month
                .entry(month_start(occurrence.date.max(today)))
                .or_default();
            if occurrence.amount.amount >= Decimal::ZERO {
                bucket.0 += occurrence.amount.amount;
            } else {
                bucket.1 -= occurrence.amount.amount;
            }
            bucket.2 += occurrence.margin.amount;
        }
    }

    let days_in_first = Decimal::from((next_month(first) - first).num_days());
    let remaining_first = Decimal::from((next_month(first) - today).num_days() - 1) / days_in_first;

    let mut balance = Scenarios {
        low: start_balance.amount,
        expected: start_balance.amount,
        high: start_balance.amount,
    };
    let months = starts
        .iter()
        .map(|&month| {
            let (income, expenses, margin) = per_month.get(&month).copied().unwrap_or_default();
            let share = if month == first {
                remaining_first
            } else {
                Decimal::ONE
            };
            let spending = |amount: Decimal| {
                (amount * share).round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
            };
            let var = variable.map_or(
                Scenarios {
                    low: Decimal::ZERO,
                    expected: Decimal::ZERO,
                    high: Decimal::ZERO,
                },
                |v| Scenarios {
                    low: spending(v.highest),
                    expected: spending(v.average),
                    high: spending(v.lowest),
                },
            );
            let net = income - expenses;
            balance = Scenarios {
                low: balance.low + net - margin - var.low,
                expected: balance.expected + net - var.expected,
                high: balance.high + net + margin - var.high,
            };
            let gap = |b: Decimal| b - start_balance.amount;
            ForecastMonth {
                month,
                planned_income: income,
                planned_expenses: expenses,
                variable_expenses: var.expected,
                balance,
                net_worth: net_worth.map(|w| Scenarios {
                    low: w.amount + gap(balance.low),
                    expected: w.amount + gap(balance.expected),
                    high: w.amount + gap(balance.high),
                }),
            }
        })
        .collect();
    Forecast {
        start_balance,
        variable,
        months,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recurrence::{Amount, AmountStep, DayOfMonth, Direction, Schedule, Window};
    use shared::{Currency, PlannedPaymentId, TransactionId};
    use transaction::TransactionSource;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
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

    fn payment(
        label: &str,
        direction: Direction,
        dom: u32,
        value: i64,
        margin: i64,
    ) -> PlannedPayment {
        PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: label.into(),
            pattern: label.to_lowercase(),
            direction,
            account_id: "compte".into(),
            category_id: None,
            schedule: Schedule::Monthly {
                day: DayOfMonth::Day(dom),
            },
            amount: Amount {
                steps: vec![AmountStep {
                    from: day(2026, 1, 1),
                    value: eur(value),
                    margin: eur(margin),
                }],
                revision: None,
                observed: false,
            },
            window: Window::default(),
        }
    }

    /// Des relevés de septembre à octobre : 300 € de courses variables par mois, plus le loyer déclaré.
    fn statements() -> Vec<Transaction> {
        let mut t = Vec::new();
        for m in [9, 10] {
            t.push(transaction(day(2026, m, 2), -300, "CARTE COURSES"));
            t.push(transaction(day(2026, m, 5), -700, "PRLV LOYER"));
            t.push(transaction(
                day(2026, m, 28),
                -(if m == 9 { 100 } else { 200 }),
                "CARTE DIVERS",
            ));
        }
        t
    }

    #[test]
    fn les_depenses_variables_excluent_les_paiements_declares() {
        let loyer = payment("Loyer", Direction::Out, 5, 700, 0);
        let v = variable_spending(
            &statements(),
            &[],
            &[loyer],
            day(2026, 11, 10),
            Currency::Eur,
        )
        .unwrap();
        // Septembre : 400 ; octobre : 500 ; le loyer déclaré n'y est pas.
        assert_eq!(
            (v.lowest, v.highest, v.average, v.months),
            (
                Decimal::from(400),
                Decimal::from(500),
                Decimal::from(450),
                2
            )
        );
    }

    #[test]
    fn projette_le_solde_mois_apres_mois_avec_trois_scenarios() {
        let loyer = payment("Loyer", Direction::Out, 5, 700, 20);
        let salaire = payment("Salaire", Direction::In, 28, 2000, 100);
        // Le salaire de novembre est arrivé : il est déjà dans le solde.
        let mut real = statements();
        real.push(transaction(day(2026, 11, 28), 2000, "VIR SALAIRE"));
        let f = forecast(
            &[loyer, salaire],
            &[],
            &real,
            &[],
            eur(1000),
            Some(eur(11_000)),
            day(2026, 11, 30), // dernier jour : plus rien à courir ce mois-ci
        );
        // Novembre : plus d'échéance à venir.
        assert_eq!(f.months.len(), HORIZON_MONTHS + 1);
        // Décembre : +2000 −700 en attendu, moins 450 variables = +850.
        let december = &f.months[1];
        assert_eq!(december.month, day(2026, 12, 1));
        assert_eq!(december.balance.expected, Decimal::from(1000 + 850));
        // Prudent : revenu −100, loyer +20, mois variable le plus cher (500) : 1000 + 2000 −700 −120 −500.
        assert_eq!(
            december.balance.low,
            Decimal::from(1000 + 2000 - 700 - 120 - 500)
        );
        // Favorable : +120 de marge, mois le moins cher (400).
        assert_eq!(
            december.balance.high,
            Decimal::from(1000 + 2000 - 700 + 120 - 400)
        );
        // Le patrimoine suit le solde, le reste ne bouge pas.
        assert_eq!(
            december.net_worth.unwrap().expected,
            Decimal::from(11_000 + 850)
        );
    }

    #[test]
    fn une_echeance_sautee_ou_honoree_n_est_pas_comptee() {
        let mut loyer = payment("Loyer", Direction::Out, 5, 700, 0);
        loyer.schedule = Schedule::Monthly {
            day: DayOfMonth::Day(5),
        };
        let exceptions = [(loyer.id.clone(), day(2026, 12, 5), ExceptionAction::Skip)];
        let f = forecast(
            &[loyer],
            &exceptions,
            &[],
            &[],
            eur(0),
            None,
            day(2026, 11, 30),
        );
        assert_eq!(f.variable, None);
        assert_eq!(f.months[1].planned_expenses, Decimal::ZERO); // décembre sauté
        assert_eq!(f.months[2].planned_expenses, Decimal::from(700)); // janvier
    }

    #[test]
    fn le_reste_du_mois_en_cours_est_proratise() {
        let f = forecast(
            &[],
            &[],
            &statements(),
            &[],
            eur(1000),
            None,
            day(2026, 11, 15),
        );
        // Novembre a 30 jours et le 15 est aujourd'hui : il en reste 15, donc la moitié de la moyenne.
        let v = f.variable.unwrap();
        let expected = (v.average * Decimal::from(15) / Decimal::from(30)).round_dp(2);
        assert_eq!(f.months[0].variable_expenses, expected);
    }

    #[test]
    fn repere_le_premier_mois_sous_un_niveau() {
        let salaire = payment("Salaire", Direction::In, 28, 100, 0);
        let f = forecast(
            &[salaire],
            &[],
            &statements(),
            &[],
            eur(100),
            None,
            day(2026, 11, 10),
        );
        let first = f.first_below(Decimal::ZERO, |s| s.expected).unwrap();
        assert!(first.balance.expected < Decimal::ZERO);
    }
}
