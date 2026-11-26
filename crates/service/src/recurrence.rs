//! Règles des paiements prévus : les reconnaître dans les relevés, en tirer un montant observé,
//! rapprocher les échéances des opérations réelles et proposer ceux qu'on n'a pas encore déclarés.
//! Des fonctions pures : paiements, exceptions et transactions déjà lus, la date du jour en paramètre.

use std::collections::{BTreeSet, HashMap};

use category::{Category, CategoryKind, normalize};
use chrono::{Datelike, Days, NaiveDate};
use recurrence::{
    Amount, AmountStep, DayOfMonth, Direction, ExceptionAction, Observed, Occurrence,
    PlannedPayment, Schedule, occurrences,
};
use rust_decimal::{Decimal, RoundingStrategy};
use shared::{AssetId, Money, PlannedPaymentId, TransactionId};
use transaction::Transaction;

/// Une opération réelle peut précéder l'échéance de si peu de jours…
pub const MATCH_DAYS_BEFORE: u64 = 3;
/// … ou la suivre de si peu (prélèvement décalé par un week-end ou un jour férié).
pub const MATCH_DAYS_AFTER: u64 = 7;
/// Le montant observé se calcule sur les derniers paiements réels, pas plus.
pub const OBSERVED_SAMPLES: usize = 6;

/// Les exceptions de tous les paiements, telles que le provider les rend.
pub type Exceptions = [(PlannedPaymentId, NaiveDate, ExceptionAction)];

/// L'opération réelle est-elle ce paiement ? Même compte, même sens, et le libellé contient le mot du paiement.
pub fn matches(payment: &PlannedPayment, transaction: &Transaction) -> bool {
    let sign_matches = match payment.direction {
        Direction::In => transaction.amount.amount > Decimal::ZERO,
        Direction::Out => transaction.amount.amount < Decimal::ZERO,
    };
    transaction.asset_id == payment.account_id
        && sign_matches
        && normalize(&transaction.label).contains(&normalize(&payment.pattern))
}

/// Les opérations réelles d'un paiement, de la plus ancienne à la plus récente.
pub fn matching_transactions<'a>(
    payment: &PlannedPayment,
    transactions: &'a [Transaction],
) -> Vec<&'a Transaction> {
    let mut found: Vec<_> = transactions
        .iter()
        .filter(|t| matches(payment, t))
        .collect();
    found.sort_by_key(|t| (t.booked_on, t.id.clone()));
    found
}

/// La valeur et la marge tirées des derniers paiements réels : **la médiane** des `OBSERVED_SAMPLES`
/// derniers montants, et pour marge **le plus grand écart** à cette médiane. `None` sans paiement réel.
pub fn observed_amount(payment: &PlannedPayment, transactions: &[Transaction]) -> Option<Observed> {
    let found = matching_transactions(payment, transactions);
    let recent = &found[found.len().saturating_sub(OBSERVED_SAMPLES)..];
    let currency = recent.last()?.amount.currency;
    let mut values: Vec<Decimal> = recent
        .iter()
        .filter(|t| t.amount.currency == currency)
        .map(|t| t.amount.amount.abs())
        .collect();
    values.sort();
    let middle = values.len() / 2;
    let median = if values.len() % 2 == 1 {
        values[middle]
    } else {
        (values[middle - 1] + values[middle]) / Decimal::TWO
    }
    .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let margin = values
        .iter()
        .map(|v| (*v - median).abs())
        .max()
        .unwrap_or_default();
    Some(Observed {
        value: Money::new(median, currency),
        margin: Money::new(margin, currency),
    })
}

/// Ce qu'est devenue une échéance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OccurrenceStatus {
    /// L'opération réelle est arrivée.
    Honored {
        transaction_id: TransactionId,
        amount: Money,
    },
    /// La date n'est pas encore (tout à fait) passée.
    Expected,
    /// La date est passée de plus de `MATCH_DAYS_AFTER` jours sans opération : prélèvement refusé,
    /// abonnement résilié… un signal à regarder.
    Missing,
    /// Sautée par une exception.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleLine {
    pub occurrence: Occurrence,
    pub status: OccurrenceStatus,
}

/// Les échéances d'un paiement entre deux dates, rapprochées des opérations réelles.
///
/// Chaque opération n'honore **qu'une** échéance : on les parcourt par date et chacune prend l'opération
/// la plus proche, dans la fenêtre `MATCH_DAYS_BEFORE` / `MATCH_DAYS_AFTER`, qui n'a pas déjà servi.
pub fn payment_lines(
    payment: &PlannedPayment,
    exceptions: &Exceptions,
    transactions: &[Transaction],
    today: NaiveDate,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<ScheduleLine> {
    let own_exceptions: Vec<_> = exceptions
        .iter()
        .filter(|(id, _, _)| id == &payment.id)
        .map(|(_, date, action)| (*date, *action))
        .collect();
    let observed = observed_amount(payment, transactions);
    let real = matching_transactions(payment, transactions);
    let mut used: BTreeSet<&TransactionId> = BTreeSet::new();

    occurrences(payment, &own_exceptions, observed, from, to)
        .into_iter()
        .map(|occurrence| {
            if occurrence.skipped {
                return ScheduleLine {
                    status: OccurrenceStatus::Skipped,
                    occurrence,
                };
            }
            let earliest = occurrence.date - Days::new(MATCH_DAYS_BEFORE);
            let latest = occurrence.date + Days::new(MATCH_DAYS_AFTER);
            let found = real
                .iter()
                .filter(|t| {
                    t.booked_on >= earliest && t.booked_on <= latest && !used.contains(&t.id)
                })
                .min_by_key(|t| (t.booked_on - occurrence.date).num_days().abs());
            let status = match found {
                Some(t) => {
                    used.insert(&t.id);
                    OccurrenceStatus::Honored {
                        transaction_id: t.id.clone(),
                        amount: t.amount,
                    }
                }
                None if today > latest => OccurrenceStatus::Missing,
                None => OccurrenceStatus::Expected,
            };
            ScheduleLine { occurrence, status }
        })
        .collect()
}

/// Une ligne du calendrier : un paiement et l'une de ses échéances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarLine {
    pub payment_id: PlannedPaymentId,
    pub line: ScheduleLine,
}

/// Le calendrier de tous les paiements autour d'aujourd'hui : les `back_days` derniers jours (honorées ou
/// manquantes) et les `ahead_days` prochains (attendues), par date.
pub fn calendar(
    payments: &[PlannedPayment],
    exceptions: &Exceptions,
    transactions: &[Transaction],
    today: NaiveDate,
    back_days: u64,
    ahead_days: u64,
) -> Vec<CalendarLine> {
    let (from, to) = (today - Days::new(back_days), today + Days::new(ahead_days));
    let mut lines: Vec<CalendarLine> = payments
        .iter()
        .flat_map(|payment| {
            payment_lines(payment, exceptions, transactions, today, from, to)
                .into_iter()
                .map(|line| CalendarLine {
                    payment_id: payment.id.clone(),
                    line,
                })
        })
        .collect();
    lines.sort_by(|a, b| {
        (a.line.occurrence.date, &a.payment_id).cmp(&(b.line.occurrence.date, &b.payment_id))
    });
    lines
}

/// Un paiement que l'historique laisse penser régulier, proposé à l'utilisateur : jamais créé d'office.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub label: String,
    /// Le mot du libellé qui reconnaît ces opérations.
    pub pattern: String,
    pub direction: Direction,
    pub account_id: AssetId,
    pub schedule: Schedule,
    pub amount: Amount,
    /// Combien d'opérations ont servi à le repérer.
    pub samples: usize,
    pub last_date: NaiveDate,
}

/// Les mots d'un libellé bancaire qui ne disent rien du bénéficiaire.
const GENERIC_WORDS: [&str; 24] = [
    "prlv",
    "prelevement",
    "vir",
    "virement",
    "sepa",
    "cb",
    "carte",
    "paiement",
    "pmt",
    "retrait",
    "dab",
    "inst",
    "ref",
    "facture",
    "fact",
    "com",
    "www",
    "pour",
    "par",
    "sur",
    "les",
    "des",
    "aux",
    "mensuel",
];

/// Les mots significatifs d'un libellé : lettres seules, trois au moins, hors mots génériques.
fn significant_words(label: &str) -> BTreeSet<String> {
    normalize(label)
        .split_whitespace()
        .filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_ascii_alphabetic()))
        .filter(|w| !GENERIC_WORDS.contains(w))
        .map(str::to_owned)
        .collect()
}

fn median_day(dates: &[NaiveDate]) -> u32 {
    let mut days: Vec<u32> = dates.iter().map(|d| d.day()).collect();
    days.sort_unstable();
    days[days.len() / 2]
}

/// Cherche, dans les opérations, ce qui revient à intervalle régulier : chaque mois, ou chaque trimestre.
///
/// Il faut **au moins trois** opérations de même compte, même sens et mêmes mots significatifs, à des
/// intervalles réguliers (25 à 35 jours, ou 85 à 95), dont la dernière est récente. Les virements internes
/// et ce que les paiements déjà déclarés reconnaissent sont écartés. Un montant stable à 2 % près donne un montant
/// fixe, sinon un montant observé (médiane des derniers).
pub fn detect(
    transactions: &[Transaction],
    categories: &[Category],
    payments: &[PlannedPayment],
) -> Vec<Suggestion> {
    let transfers: BTreeSet<_> = categories
        .iter()
        .filter(|c| c.kind == CategoryKind::Transfer)
        .map(|c| &c.id)
        .collect();
    let Some(newest) = transactions.iter().map(|t| t.booked_on).max() else {
        return Vec::new();
    };

    let mut groups: HashMap<(AssetId, bool, String), Vec<&Transaction>> = HashMap::new();
    for t in transactions {
        if t.category_id
            .as_ref()
            .is_some_and(|c| transfers.contains(c))
            || t.amount.amount.is_zero()
        {
            continue;
        }
        let words = significant_words(&t.label);
        if words.is_empty() {
            continue;
        }
        let key = words.into_iter().collect::<Vec<_>>().join(" ");
        groups
            .entry((t.asset_id.clone(), t.amount.amount.is_sign_positive(), key))
            .or_default()
            .push(t);
    }

    let mut suggestions: Vec<Suggestion> = groups
        .into_iter()
        .filter_map(|((account_id, incoming, key), mut group)| {
            if group.len() < 3 {
                return None;
            }
            group.sort_by_key(|t| t.booked_on);
            let dates: Vec<NaiveDate> = group.iter().map(|t| t.booked_on).collect();
            let gaps: Vec<i64> = dates.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
            let (every, max_age) = if gaps.iter().all(|g| (25..=35).contains(g)) {
                (1, 70)
            } else if gaps.iter().all(|g| (85..=95).contains(g)) {
                (3, 130)
            } else {
                return None;
            };
            let last_date = *dates.last()?;
            if (newest - last_date).num_days() > max_age {
                return None;
            }
            let direction = if incoming {
                Direction::In
            } else {
                Direction::Out
            };
            let words: Vec<&str> = key.split(' ').collect();
            let covered = payments.iter().any(|p| {
                let pattern = normalize(&p.pattern);
                p.account_id == account_id
                    && p.direction == direction
                    && words
                        .iter()
                        .any(|w| w.contains(&pattern) || pattern.contains(w))
            });
            if covered {
                return None;
            }
            let pattern = words
                .iter()
                .max_by_key(|w| (w.len(), std::cmp::Reverse(**w)))?
                .to_string();
            let day = DayOfMonth::Day(median_day(&dates));
            let schedule = if every == 1 {
                Schedule::Monthly { day }
            } else {
                Schedule::EveryNMonths {
                    n: 3,
                    day,
                    first_month: dates[0].with_day(1)?,
                }
            };
            let currency = group.last()?.amount.currency;
            let mut values: Vec<Decimal> = group.iter().map(|t| t.amount.amount.abs()).collect();
            values.sort();
            let median = values[values.len() / 2];
            let stable = median.is_zero()
                || (values[values.len() - 1] - values[0]) / median <= Decimal::new(2, 2);
            let latest = group.last()?.amount.amount.abs();
            let amount = Amount {
                steps: vec![AmountStep {
                    from: dates[0].with_day(1)?,
                    value: Money::new(if stable { latest } else { median }, currency),
                    margin: Money::zero(currency),
                }],
                revision: None,
                observed: !stable,
            };
            let mut label = pattern.clone();
            label[..1].make_ascii_uppercase();
            Some(Suggestion {
                label,
                pattern,
                direction,
                account_id,
                schedule,
                amount,
                samples: group.len(),
                last_date,
            })
        })
        .collect();
    suggestions.sort_by(|a, b| {
        (a.direction == Direction::Out, &a.pattern)
            .cmp(&(b.direction == Direction::Out, &b.pattern))
    });
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;
    use recurrence::{Revision, Window};
    use shared::Currency;
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
        pattern: &str,
        direction: Direction,
        value: i64,
        margin: i64,
        observed: bool,
    ) -> PlannedPayment {
        PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: pattern.into(),
            pattern: pattern.into(),
            direction,
            account_id: "compte".into(),
            category_id: None,
            schedule: Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            amount: Amount {
                steps: vec![AmountStep {
                    from: day(2026, 1, 1),
                    value: eur(value),
                    margin: eur(margin),
                }],
                revision: None,
                observed,
            },
            window: Window::default(),
        }
    }

    // ── Reconnaissance ──

    #[test]
    fn reconnait_une_operation_par_compte_sens_et_mot_du_libelle() {
        let p = payment("edf", Direction::Out, 80, 0, false);
        assert!(matches(
            &p,
            &transaction(day(2026, 11, 5), -80, "PRLV  EDF  CLIENTS")
        ));
        assert!(!matches(&p, &transaction(day(2026, 11, 5), 80, "PRLV EDF"))); // un remboursement
        assert!(!matches(
            &p,
            &transaction(day(2026, 11, 5), -80, "PRLV GAZ")
        ));
        let mut other = transaction(day(2026, 11, 5), -80, "PRLV EDF");
        other.asset_id = "autre".into();
        assert!(!matches(&p, &other));
    }

    // ── Montant observé ──

    #[test]
    fn le_montant_observe_est_la_mediane_des_derniers_paiements_et_la_marge_l_ecart_maximal() {
        let p = payment("salaire", Direction::In, 1800, 0, true);
        let transactions: Vec<_> = [1750, 1900, 1800, 2000, 1850]
            .into_iter()
            .enumerate()
            .map(|(i, amount)| transaction(day(2026, 6 + i as u32, 28), amount, "VIR SALAIRE ACME"))
            .collect();
        let observed = observed_amount(&p, &transactions).unwrap();
        assert_eq!(observed.value, eur(1850)); // médiane de 1750, 1800, 1850, 1900, 2000
        assert_eq!(observed.margin, eur(150)); // 2000 - 1850
    }

    #[test]
    fn le_montant_observe_ne_regarde_que_les_six_derniers_paiements() {
        let p = payment("salaire", Direction::In, 1, 0, true);
        let mut transactions: Vec<_> = (1..=3)
            .map(|m| transaction(day(2025, m, 28), 9999, "VIR SALAIRE"))
            .collect();
        transactions.extend((1..=6).map(|m| transaction(day(2026, m, 28), 2000, "VIR SALAIRE")));
        let observed = observed_amount(&p, &transactions).unwrap();
        assert_eq!((observed.value, observed.margin), (eur(2000), eur(0)));
    }

    #[test]
    fn la_mediane_de_deux_valeurs_est_leur_moyenne_et_sans_paiement_reel_rien() {
        let p = payment("salaire", Direction::In, 1, 0, true);
        let transactions = [
            transaction(day(2026, 5, 28), 1000, "VIR SALAIRE"),
            transaction(day(2026, 6, 28), 2000, "VIR SALAIRE"),
        ];
        assert_eq!(observed_amount(&p, &transactions).unwrap().value, eur(1500));
        assert_eq!(observed_amount(&p, &[]), None);
    }

    // ── Rapprochement ──

    fn lines(
        p: &PlannedPayment,
        transactions: &[Transaction],
        today: NaiveDate,
    ) -> Vec<(NaiveDate, OccurrenceStatus)> {
        payment_lines(
            p,
            &[],
            transactions,
            today,
            day(2026, 8, 1),
            day(2026, 11, 30),
        )
        .into_iter()
        .map(|l| (l.occurrence.date, l.status))
        .collect()
    }

    #[test]
    fn une_echeance_est_honoree_attendue_ou_manquante() {
        let p = payment("loyer", Direction::Out, 700, 0, false);
        let aug = transaction(day(2026, 8, 6), -700, "PRLV LOYER"); // un jour après la date
        let transactions = [aug.clone()];
        let result = lines(&p, &transactions, day(2026, 11, 3));

        assert_eq!(
            result[0],
            (
                day(2026, 8, 5),
                OccurrenceStatus::Honored {
                    transaction_id: aug.id.clone(),
                    amount: eur(-700)
                }
            )
        );
        // Septembre et octobre : la date est passée de plus de 7 jours sans opération.
        assert_eq!(result[1], (day(2026, 9, 5), OccurrenceStatus::Missing));
        assert_eq!(result[2], (day(2026, 10, 5), OccurrenceStatus::Missing));
        // Novembre : le 5 n'est pas encore là.
        assert_eq!(result[3], (day(2026, 11, 5), OccurrenceStatus::Expected));
    }

    #[test]
    fn une_echeance_reste_attendue_pendant_la_fenetre_de_tolerance() {
        let p = payment("loyer", Direction::Out, 700, 0, false);
        // Le 5 du mois, aujourd'hui le 12 : 7 jours après, encore attendue ; le 13, manquante.
        assert_eq!(
            lines(&p, &[], day(2026, 11, 12)).last().unwrap().1,
            OccurrenceStatus::Expected
        );
        assert_eq!(
            lines(&p, &[], day(2026, 11, 13)).last().unwrap().1,
            OccurrenceStatus::Missing
        );
    }

    #[test]
    fn une_operation_n_honore_qu_une_echeance() {
        // Un loyer mensuel, une seule opération réelle entre deux échéances proches de ses dates.
        let mut p = payment("loyer", Direction::Out, 700, 0, false);
        p.schedule = Schedule::EveryNMonths {
            n: 2,
            day: DayOfMonth::Day(5),
            first_month: day(2026, 1, 1),
        };
        let one = transaction(day(2026, 9, 5), -700, "PRLV LOYER");
        let honored = lines(&p, &[one], day(2026, 11, 30))
            .into_iter()
            .filter(|(_, s)| matches!(s, OccurrenceStatus::Honored { .. }))
            .count();
        assert_eq!(honored, 1);
    }

    #[test]
    fn une_echeance_sautee_n_est_ni_honoree_ni_manquante() {
        let p = payment("loyer", Direction::Out, 700, 0, false);
        let exceptions = [(p.id.clone(), day(2026, 9, 5), ExceptionAction::Skip)];
        let result: Vec<_> = payment_lines(
            &p,
            &exceptions,
            &[],
            day(2026, 11, 3),
            day(2026, 9, 1),
            day(2026, 9, 30),
        )
        .into_iter()
        .map(|l| l.status)
        .collect();
        assert_eq!(result, [OccurrenceStatus::Skipped]);
    }

    #[test]
    fn un_salaire_observe_prevoit_la_mediane_reelle() {
        let p = payment("salaire", Direction::In, 1000, 0, true);
        let transactions: Vec<_> = [1800, 2000, 1900]
            .into_iter()
            .enumerate()
            .map(|(i, a)| transaction(day(2026, 8 + i as u32, 28), a, "VIR SALAIRE ACME"))
            .collect();
        let december = payment_lines(
            &p,
            &[],
            &transactions,
            day(2026, 11, 3),
            day(2026, 12, 1),
            day(2026, 12, 31),
        );
        assert_eq!(december[0].occurrence.amount, eur(1900));
        assert_eq!(december[0].occurrence.margin, eur(100));
        assert!(december[0].occurrence.estimated);
    }

    #[test]
    fn le_calendrier_melange_les_paiements_par_date() {
        let loyer = payment("loyer", Direction::Out, 700, 0, false);
        let mut abo = payment("netflix", Direction::Out, 12, 0, false);
        abo.schedule = Schedule::Monthly {
            day: DayOfMonth::Day(2),
        };
        let result = calendar(
            &[loyer.clone(), abo.clone()],
            &[],
            &[],
            day(2026, 11, 3),
            10,
            40,
        );
        let order: Vec<_> = result
            .iter()
            .map(|l| (l.line.occurrence.date, l.payment_id == abo.id))
            .collect();
        // Le 2 novembre (abonnement), le 5 novembre (loyer), le 2 décembre (abonnement), le 5 décembre (loyer).
        assert_eq!(
            order,
            [
                (day(2026, 11, 2), true),
                (day(2026, 11, 5), false),
                (day(2026, 12, 2), true),
                (day(2026, 12, 5), false)
            ]
        );
    }

    #[test]
    fn la_revision_annuelle_se_voit_dans_le_calendrier() {
        let mut p = payment("fermage", Direction::In, 1000, 0, false);
        p.amount.revision = Some(Revision {
            month: 10,
            day: 15,
            change: Decimal::from(2),
        });
        let result = payment_lines(
            &p,
            &[],
            &[],
            day(2026, 11, 3),
            day(2026, 11, 1),
            day(2026, 11, 30),
        );
        assert!(result[0].occurrence.estimated);
        assert_eq!(
            result[0].occurrence.amount,
            Money::new(Decimal::new(102000, 2), Currency::Eur)
        );
    }

    // ── Détection ──

    fn monthly(
        label: &str,
        amounts: &[i64],
        day_of_month: u32,
        first_month: u32,
    ) -> Vec<Transaction> {
        amounts
            .iter()
            .enumerate()
            .map(|(i, a)| transaction(day(2026, first_month + i as u32, day_of_month), *a, label))
            .collect()
    }

    #[test]
    fn detecte_un_paiement_mensuel_de_montant_stable() {
        let transactions = monthly("PRLV SEPA NETFLIX 0412", &[-12, -12, -12, -12], 9, 6);
        let found = detect(&transactions, &[], &[]);
        assert_eq!(found.len(), 1);
        let s = &found[0];
        assert_eq!(
            (s.pattern.as_str(), s.label.as_str()),
            ("netflix", "Netflix")
        );
        assert_eq!(s.direction, Direction::Out);
        assert_eq!(
            s.schedule,
            Schedule::Monthly {
                day: DayOfMonth::Day(9)
            }
        );
        assert!(!s.amount.observed);
        assert_eq!(s.amount.steps[0].value, eur(12));
        assert_eq!(s.samples, 4);
    }

    #[test]
    fn un_montant_qui_varie_devient_un_montant_observe() {
        let transactions = monthly("VIR SALAIRE ACME", &[1800, 2000, 1900, 1950], 28, 6);
        let s = &detect(&transactions, &[], &[])[0];
        assert_eq!(s.direction, Direction::In);
        assert!(s.amount.observed);
        assert_eq!(s.amount.steps[0].value, eur(1950));
    }

    #[test]
    fn detecte_un_rythme_trimestriel() {
        let transactions = [
            transaction(day(2026, 2, 10), 4000, "VIR FERMAGE DUPONT"),
            transaction(day(2026, 5, 11), 4000, "VIR FERMAGE DUPONT"),
            transaction(day(2026, 8, 10), 4000, "VIR FERMAGE DUPONT"),
            transaction(day(2026, 11, 10), 4000, "VIR FERMAGE DUPONT"),
        ];
        let s = &detect(&transactions, &[], &[])[0];
        assert_eq!(s.pattern, "fermage");
        assert_eq!(
            s.schedule,
            Schedule::EveryNMonths {
                n: 3,
                day: DayOfMonth::Day(10),
                first_month: day(2026, 2, 1)
            }
        );
    }

    #[test]
    fn ne_propose_ni_irregulier_ni_trop_court_ni_deja_declare_ni_termine() {
        // Moins de trois opérations.
        assert!(detect(&monthly("EDF", &[-50, -50], 5, 9), &[], &[]).is_empty());
        // Intervalles irréguliers.
        let irregular = [
            transaction(day(2026, 6, 5), -50, "EDF"),
            transaction(day(2026, 6, 20), -50, "EDF"),
            transaction(day(2026, 9, 1), -50, "EDF"),
        ];
        assert!(detect(&irregular, &[], &[]).is_empty());
        // Déjà déclaré.
        let declared = payment("edf", Direction::Out, 50, 0, false);
        assert!(
            detect(
                &monthly("PRLV EDF", &[-50, -50, -50], 5, 8),
                &[],
                &[declared]
            )
            .is_empty()
        );
        // Terminé : la dernière opération date de plus de 70 jours avant la plus récente du relevé.
        let mut stopped = monthly("ANCIEN ABONNEMENT", &[-9, -9, -9], 5, 1);
        stopped.push(transaction(day(2026, 11, 20), -3, "AUTRE CHOSE"));
        assert!(detect(&stopped, &[], &[]).is_empty());
    }

    #[test]
    fn ne_propose_pas_les_virements_internes() {
        let categories = [Category {
            id: "livret".into(),
            name: "Virement interne".into(),
            kind: CategoryKind::Transfer,
        }];
        let mut transactions = monthly("VIR EPARGNE LIVRET", &[-300, -300, -300], 3, 8);
        for t in &mut transactions {
            t.category_id = Some("livret".into());
        }
        assert!(detect(&transactions, &categories, &[]).is_empty());
    }

    #[test]
    fn distingue_deux_beneficiaires_et_deux_comptes() {
        let mut transactions = monthly("PRLV EDF", &[-50, -50, -50], 5, 8);
        transactions.extend(monthly("PRLV GAZ", &[-30, -30, -30], 12, 8));
        let mut other_account = monthly("PRLV EDF", &[-50, -50, -50], 5, 8);
        for t in &mut other_account {
            t.asset_id = "autre".into();
        }
        transactions.extend(other_account);
        assert_eq!(detect(&transactions, &[], &[]).len(), 3);
    }
}
