//! Règles du budget : la synthèse d'un mois, l'état des enveloppes, le matelas de sécurité
//! et le versement d'épargne suggéré. Des fonctions pures : transactions, catégories et enveloppes
//! déjà lues en entrée, la date du jour en paramètre.
//!
//! Un budget est tenu **par devise** : une transaction dans une autre devise que celle demandée
//! est ignorée (aujourd'hui, seul l'euro existe).

use std::collections::HashMap;

use budget::{Envelope, effective_limit, month_start, next_month, previous_month};
use category::{Category, CategoryKind};
use chrono::NaiveDate;
use rust_decimal::{Decimal, RoundingStrategy};
use shared::{CategoryId, Currency, Money};
use transaction::Transaction;

/// Combien de mois complets l'évaluation du matelas regarde en arrière.
pub const CUSHION_MONTHS: u32 = 3;
/// Le matelas est arrondi au multiple supérieur de ce montant (des chiffres lisibles).
const CUSHION_ROUNDING: i64 = 50;
/// À partir de cette part du plafond, une enveloppe est signalée « proche ».
const NEAR_PERCENT: Decimal = Decimal::from_parts(80, 0, 0, false, 2); // 0,80

fn in_month(date: NaiveDate, month: NaiveDate) -> bool {
    month_start(date) == month_start(month)
}

fn kinds(categories: &[Category]) -> HashMap<&CategoryId, CategoryKind> {
    categories.iter().map(|c| (&c.id, c.kind)).collect()
}

/// Ce qui s'est passé sur les comptes bancaires pendant un mois.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthSummary {
    /// Les revenus classés en catégorie de revenu.
    pub income: Decimal,
    /// Les dépenses classées en catégorie de dépense, nettes des remboursements.
    pub expenses: Decimal,
    /// Les sorties d'argent sans catégorie (positif) : elles comptent dans les dépenses,
    /// puisqu'on ne sait pas ce qu'elles sont.
    pub uncategorized_outflow: Decimal,
    /// Le nombre d'opérations sans catégorie, pour inviter à les classer.
    pub uncategorized_count: usize,
    /// Ce qui est sorti vers l'épargne (virements internes, net) : ni revenu ni dépense.
    pub transferred_out: Decimal,
}

impl MonthSummary {
    /// Tout ce qui est sorti du budget : dépenses classées et sorties sans catégorie.
    pub fn spending(&self) -> Decimal {
        self.expenses + self.uncategorized_outflow
    }

    /// Revenus moins dépenses : ce qu'il reste du mois (négatif si l'on a dépensé plus que gagné).
    /// Les entrées sans catégorie ne comptent pas : on ne sait pas si c'est un revenu.
    pub fn result(&self) -> Decimal {
        self.income - self.spending()
    }
}

pub fn month_summary(
    transactions: &[Transaction],
    categories: &[Category],
    month: NaiveDate,
    currency: Currency,
) -> MonthSummary {
    let kinds = kinds(categories);
    let mut summary = MonthSummary {
        income: Decimal::ZERO,
        expenses: Decimal::ZERO,
        uncategorized_outflow: Decimal::ZERO,
        uncategorized_count: 0,
        transferred_out: Decimal::ZERO,
    };
    for t in transactions
        .iter()
        .filter(|t| t.amount.currency == currency && in_month(t.booked_on, month))
    {
        let amount = t.amount.amount;
        match t.category_id.as_ref().and_then(|id| kinds.get(id)) {
            Some(CategoryKind::Income) => summary.income += amount,
            Some(CategoryKind::Expense) => summary.expenses -= amount,
            Some(CategoryKind::Transfer) => summary.transferred_out -= amount,
            None => {
                summary.uncategorized_count += 1;
                if amount.is_sign_negative() {
                    summary.uncategorized_outflow -= amount;
                }
            }
        }
    }
    summary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeState {
    /// Sous 80 % du plafond.
    Within,
    /// De 80 % au plafond inclus.
    Near,
    /// Au-delà du plafond.
    Over,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeStatus {
    pub category: Category,
    pub limit: Money,
    /// Dépensé dans le mois, net des remboursements, jamais négatif.
    pub spent: Money,
    /// Plafond moins dépensé : négatif en cas de dépassement.
    pub remaining: Money,
    /// La part du plafond consommée, arrondie à l'unité (peut dépasser 100).
    pub percent: u32,
    pub state: EnvelopeState,
}

/// L'état des enveloppes d'un mois : une ligne par catégorie de dépense qui a un plafond ce mois-là,
/// dans l'ordre des catégories.
pub fn envelope_statuses(
    envelopes: &[Envelope],
    categories: &[Category],
    transactions: &[Transaction],
    month: NaiveDate,
    currency: Currency,
) -> Vec<EnvelopeStatus> {
    let mut spent: HashMap<&CategoryId, Decimal> = HashMap::new();
    for t in transactions
        .iter()
        .filter(|t| t.amount.currency == currency && in_month(t.booked_on, month))
    {
        if let Some(id) = &t.category_id {
            *spent.entry(id).or_default() -= t.amount.amount;
        }
    }
    categories
        .iter()
        .filter(|c| c.kind == CategoryKind::Expense)
        .filter_map(|category| {
            let limit = effective_limit(envelopes, &category.id, month)?;
            if limit.currency != currency {
                return None;
            }
            let spent = spent
                .get(&category.id)
                .copied()
                .unwrap_or_default()
                .max(Decimal::ZERO);
            let state = if spent > limit.amount {
                EnvelopeState::Over
            } else if spent >= limit.amount * NEAR_PERCENT && spent > Decimal::ZERO {
                EnvelopeState::Near
            } else {
                EnvelopeState::Within
            };
            let percent = if limit.amount.is_zero() {
                if spent.is_zero() { 0 } else { 100 }
            } else {
                (spent * Decimal::ONE_HUNDRED / limit.amount)
                    .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
                    .try_into()
                    .unwrap_or(u32::MAX)
            };
            Some(EnvelopeStatus {
                category: category.clone(),
                limit,
                spent: Money::new(spent, currency),
                remaining: Money::new(limit.amount - spent, currency),
                percent,
                state,
            })
        })
        .collect()
}

/// Le matelas de sécurité évalué d'après la situation réelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cushion {
    pub amount: Money,
    /// Combien de mois complets l'évaluation a pu regarder (de 1 à `CUSHION_MONTHS`).
    pub months: usize,
}

/// Les derniers mois complets (jusqu'à `CUSHION_MONTHS`), du plus récent au plus ancien : un mois est
/// **complet** si les relevés importés le couvrent, c'est-à-dire une opération dans ses 7 premiers jours
/// et une dans ses 7 derniers. Le mois en cours ne l'est jamais.
pub fn complete_months(
    transactions: &[Transaction],
    today: NaiveDate,
    currency: Currency,
) -> Vec<NaiveDate> {
    let mut dates = transactions
        .iter()
        .filter(|t| t.amount.currency == currency)
        .map(|t| t.booked_on);
    let Some(first) = dates.next() else {
        return Vec::new();
    };
    let (first, last) = dates.fold((first, first), |(lo, hi), d| (lo.min(d), hi.max(d)));

    let mut month = month_start(today);
    let mut complete = Vec::new();
    for _ in 0..CUSHION_MONTHS {
        month = previous_month(month);
        let Some(end) = next_month(month).pred_opt() else {
            break;
        };
        if first <= month + chrono::Days::new(6) && last >= end - chrono::Days::new(6) {
            complete.push(month);
        }
    }
    complete
}

/// Le matelas qu'il faut garder sur les comptes courants : **le mois le plus cher** parmi les derniers
/// mois complets (jusqu'à `CUSHION_MONTHS`), arrondi aux 50 € supérieurs. De quoi traverser un mois
/// aussi coûteux sans revenu, sans passer à découvert. Les virements internes ne comptent pas.
/// `None` tant qu'aucun mois n'est complet.
pub fn recommended_cushion(
    transactions: &[Transaction],
    categories: &[Category],
    today: NaiveDate,
    currency: Currency,
) -> Option<Cushion> {
    let costs: Vec<Decimal> = complete_months(transactions, today, currency)
        .into_iter()
        .map(|month| month_summary(transactions, categories, month, currency).spending())
        .collect();
    let highest = costs.iter().copied().max()?;
    let step = Decimal::from(CUSHION_ROUNDING);
    let rounded = (highest / step).ceil() * step;
    Some(Cushion {
        amount: Money::new(rounded.max(Decimal::ZERO), currency),
        months: costs.len(),
    })
}

/// Ce qui limite le versement suggéré.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavingsLimit {
    /// Ce qu'il est resté du mois, déduction faite de ce qui a déjà été versé.
    Surplus,
    /// Le matelas : le compte ne doit pas passer dessous.
    Cushion,
    /// Le plafond du livret.
    Ceiling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavingsSuggestion {
    /// Le montant à verser (zéro si rien n'est à verser).
    pub amount: Money,
    /// Ce qu'il est resté du mois : revenus moins dépenses, jamais négatif.
    pub surplus: Money,
    /// Ce qui a déjà été viré vers l'épargne ce mois-là.
    pub already_saved: Money,
    /// Ce que les comptes courants contiennent au-delà du matelas.
    pub above_cushion: Money,
    /// Ce que le livret peut encore recevoir avant son plafond.
    pub room: Money,
    /// Ce qui borne le montant.
    pub limited_by: SavingsLimit,
}

/// Le versement d'épargne à suggérer pour un mois : **le plus petit** de
/// - ce qu'il est resté du mois, moins ce qui a déjà été versé ;
/// - ce que les comptes courants contiennent au-delà du matelas ;
/// - la place restante sous le plafond du livret.
///
/// Prendre le plus petit des trois évite tout versement qui ferait passer sous le matelas ou
/// au-dessus du plafond, et se corrige tout seul : une fois le virement fait, le solde baisse et
/// `already_saved` monte, donc la suggestion tombe à zéro.
pub fn savings_suggestion(
    summary: &MonthSummary,
    current_balance: Money,
    cushion: Money,
    savings_balance: Money,
    ceiling: Money,
) -> SavingsSuggestion {
    let currency = current_balance.currency;
    let money = |amount: Decimal| Money::new(amount.max(Decimal::ZERO), currency);
    let surplus = summary.result().max(Decimal::ZERO);
    let already_saved = summary.transferred_out.max(Decimal::ZERO);
    let left_of_month = (surplus - already_saved).max(Decimal::ZERO);
    let above_cushion = (current_balance.amount - cushion.amount).max(Decimal::ZERO);
    let room = (ceiling.amount - savings_balance.amount).max(Decimal::ZERO);

    // À égalité, le plafond puis le matelas passent avant : ce sont les raisons les plus utiles à lire.
    let (amount, limited_by) = [
        (room, SavingsLimit::Ceiling),
        (above_cushion, SavingsLimit::Cushion),
        (left_of_month, SavingsLimit::Surplus),
    ]
    .into_iter()
    .min_by_key(|(amount, _)| *amount)
    .expect("trois bornes");
    SavingsSuggestion {
        amount: money(amount),
        surplus: money(surplus),
        already_saved: money(already_saved),
        above_cushion: money(above_cushion),
        room: money(room),
        limited_by,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use budget::Envelope;
    use shared::{EnvelopeId, TransactionId};
    use transaction::TransactionSource;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn category(id: &str, kind: CategoryKind) -> Category {
        Category {
            id: id.into(),
            name: id.into(),
            kind,
        }
    }

    fn categories() -> Vec<Category> {
        vec![
            category("courses", CategoryKind::Expense),
            category("loyer", CategoryKind::Expense),
            category("salaire", CategoryKind::Income),
            category("livret", CategoryKind::Transfer),
        ]
    }

    fn transaction(date: NaiveDate, amount: i64, category: Option<&str>) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: "a".into(),
            booked_on: date,
            amount: eur(amount),
            label: "x".into(),
            source: TransactionSource::Csv,
            category_id: category.map(Into::into),
        }
    }

    fn envelope(category: &str, limit: i64) -> Envelope {
        Envelope {
            id: EnvelopeId::generate(),
            category_id: category.into(),
            limit: Some(eur(limit)),
            from_month: day(2026, 1, 1),
        }
    }

    // ── Synthèse du mois ──

    #[test]
    fn resume_un_mois_en_separant_revenus_depenses_epargne_et_inconnu() {
        let transactions = [
            transaction(day(2026, 10, 1), 2000, Some("salaire")),
            transaction(day(2026, 10, 5), -700, Some("loyer")),
            transaction(day(2026, 10, 9), -150, Some("courses")),
            transaction(day(2026, 10, 12), 20, Some("courses")), // un remboursement
            transaction(day(2026, 10, 15), -300, Some("livret")),
            transaction(day(2026, 10, 20), -40, None),
            transaction(day(2026, 10, 21), 15, None), // entrée inconnue : ignorée du résultat
            transaction(day(2026, 11, 2), -999, Some("courses")), // un autre mois
        ];
        let summary = month_summary(
            &transactions,
            &categories(),
            day(2026, 10, 17),
            Currency::Eur,
        );

        assert_eq!(summary.income, Decimal::from(2000));
        assert_eq!(summary.expenses, Decimal::from(830)); // 700 + 150 - 20
        assert_eq!(summary.uncategorized_outflow, Decimal::from(40));
        assert_eq!(summary.uncategorized_count, 2);
        assert_eq!(summary.transferred_out, Decimal::from(300));
        assert_eq!(summary.spending(), Decimal::from(870));
        assert_eq!(summary.result(), Decimal::from(1130));
    }

    #[test]
    fn un_virement_interne_n_est_ni_un_revenu_ni_une_depense() {
        let transactions = [
            transaction(day(2026, 10, 3), -500, Some("livret")),
            transaction(day(2026, 10, 3), 500, Some("livret")),
        ];
        let summary = month_summary(
            &transactions,
            &categories(),
            day(2026, 10, 1),
            Currency::Eur,
        );
        assert_eq!(summary.result(), Decimal::ZERO);
        assert_eq!(summary.transferred_out, Decimal::ZERO);
    }

    // ── Enveloppes ──

    fn status_of(spent: i64, limit: i64) -> EnvelopeStatus {
        let transactions = [transaction(day(2026, 10, 5), -spent, Some("courses"))];
        let statuses = envelope_statuses(
            &[envelope("courses", limit)],
            &categories(),
            &transactions,
            day(2026, 10, 1),
            Currency::Eur,
        );
        assert_eq!(statuses.len(), 1);
        statuses.into_iter().next().unwrap()
    }

    #[test]
    fn une_enveloppe_passe_de_dans_les_clous_a_proche_puis_depassee() {
        assert_eq!(status_of(100, 400).state, EnvelopeState::Within);
        assert_eq!(status_of(319, 400).state, EnvelopeState::Within);
        assert_eq!(status_of(320, 400).state, EnvelopeState::Near); // 80 % pile
        assert_eq!(status_of(400, 400).state, EnvelopeState::Near); // le plafond inclus
        let over = status_of(450, 400);
        assert_eq!(over.state, EnvelopeState::Over);
        assert_eq!(over.remaining, eur(-50));
        assert_eq!(over.percent, 113); // 112,5 arrondi
    }

    #[test]
    fn le_reste_et_le_pourcentage_se_calculent_sur_le_depense_net() {
        let status = status_of(100, 400);
        assert_eq!(status.spent, eur(100));
        assert_eq!(status.remaining, eur(300));
        assert_eq!(status.percent, 25);
    }

    #[test]
    fn un_remboursement_superieur_a_la_depense_ne_rend_pas_le_depense_negatif() {
        let transactions = [transaction(day(2026, 10, 5), 50, Some("courses"))];
        let statuses = envelope_statuses(
            &[envelope("courses", 400)],
            &categories(),
            &transactions,
            day(2026, 10, 1),
            Currency::Eur,
        );
        assert_eq!(statuses[0].spent, eur(0));
        assert_eq!(statuses[0].state, EnvelopeState::Within);
    }

    #[test]
    fn seules_les_categories_de_depense_avec_un_plafond_ce_mois_la_ont_une_ligne() {
        let envelopes = [
            envelope("courses", 400),
            envelope("salaire", 100), // un revenu n'a pas d'enveloppe
        ];
        let statuses = envelope_statuses(
            &envelopes,
            &categories(),
            &[],
            day(2026, 10, 1),
            Currency::Eur,
        );
        let names: Vec<_> = statuses.iter().map(|s| s.category.name.as_str()).collect();
        assert_eq!(names, ["courses"]);
        // Avant le début de l'enveloppe, aucune ligne.
        assert!(
            envelope_statuses(
                &envelopes,
                &categories(),
                &[],
                day(2025, 12, 1),
                Currency::Eur
            )
            .is_empty()
        );
    }

    #[test]
    fn un_plafond_a_zero_est_depasse_des_la_premiere_depense() {
        let status = status_of(1, 0);
        assert_eq!(status.state, EnvelopeState::Over);
        assert_eq!(status.percent, 100);
    }

    // ── Matelas ──

    /// Un relevé qui couvre les mois de `from` à `to` inclus : une dépense au début et à la fin de chaque mois.
    fn statement(months: std::ops::RangeInclusive<u32>, monthly_total: i64) -> Vec<Transaction> {
        months
            .flat_map(|m| {
                [
                    transaction(day(2026, m, 2), -monthly_total / 2, Some("loyer")),
                    transaction(
                        day(2026, m, 28),
                        -(monthly_total - monthly_total / 2),
                        Some("courses"),
                    ),
                ]
            })
            .collect()
    }

    #[test]
    fn le_matelas_est_le_mois_le_plus_cher_des_trois_derniers_arrondi_aux_50_euros() {
        let mut transactions = statement(7..=9, 1000);
        transactions.extend(statement(10..=10, 1234)); // le plus cher
        // En novembre, les 3 derniers mois complets sont août, septembre et octobre.
        let cushion = recommended_cushion(
            &transactions,
            &categories(),
            day(2026, 11, 20),
            Currency::Eur,
        )
        .unwrap();
        assert_eq!(cushion.amount, eur(1250)); // 1 234 arrondi aux 50 € supérieurs
        assert_eq!(cushion.months, 3);
    }

    #[test]
    fn le_matelas_ignore_les_mois_anterieurs_a_la_fenetre() {
        let mut transactions = statement(1..=1, 5000); // janvier : hors des 3 derniers mois
        transactions.extend(statement(8..=10, 800));
        let cushion = recommended_cushion(
            &transactions,
            &categories(),
            day(2026, 11, 20),
            Currency::Eur,
        )
        .unwrap();
        assert_eq!(cushion.amount, eur(800));
    }

    #[test]
    fn le_matelas_ne_compte_pas_les_virements_internes() {
        let mut transactions = statement(10..=10, 1000);
        transactions.push(transaction(day(2026, 10, 15), -5000, Some("livret")));
        let cushion = recommended_cushion(
            &transactions,
            &categories(),
            day(2026, 11, 20),
            Currency::Eur,
        )
        .unwrap();
        assert_eq!(cushion.amount, eur(1000));
    }

    #[test]
    fn le_mois_en_cours_et_un_mois_partiel_ne_donnent_pas_de_matelas() {
        // Seul octobre est couvert ; novembre est en cours.
        let transactions = statement(11..=11, 1000);
        assert_eq!(
            recommended_cushion(
                &transactions,
                &categories(),
                day(2026, 11, 20),
                Currency::Eur
            ),
            None
        );
        // Un relevé qui ne couvre qu'une partie d'octobre (du 10 au 20) n'est pas un mois complet.
        let partial = [
            transaction(day(2026, 10, 10), -100, Some("courses")),
            transaction(day(2026, 10, 20), -100, Some("courses")),
        ];
        assert_eq!(
            recommended_cushion(&partial, &categories(), day(2026, 11, 20), Currency::Eur),
            None
        );
        assert_eq!(
            recommended_cushion(&[], &categories(), day(2026, 11, 20), Currency::Eur),
            None
        );
    }

    // ── Versement d'épargne ──

    fn summary(income: i64, spending: i64, transferred: i64) -> MonthSummary {
        MonthSummary {
            income: Decimal::from(income),
            expenses: Decimal::from(spending),
            uncategorized_outflow: Decimal::ZERO,
            uncategorized_count: 0,
            transferred_out: Decimal::from(transferred),
        }
    }

    #[test]
    fn suggere_le_restant_du_mois_quand_rien_ne_le_borne() {
        let s = savings_suggestion(
            &summary(2000, 1500, 0),
            eur(5000),
            eur(1000),
            eur(0),
            eur(22_950),
        );
        assert_eq!(s.amount, eur(500));
        assert_eq!(s.limited_by, SavingsLimit::Surplus);
    }

    #[test]
    fn le_matelas_borne_le_versement() {
        // 500 € de reste, mais 300 € seulement au-dessus du matelas.
        let s = savings_suggestion(
            &summary(2000, 1500, 0),
            eur(1300),
            eur(1000),
            eur(0),
            eur(22_950),
        );
        assert_eq!(s.amount, eur(300));
        assert_eq!(s.limited_by, SavingsLimit::Cushion);
    }

    #[test]
    fn rien_a_verser_quand_le_compte_est_sous_le_matelas() {
        let s = savings_suggestion(
            &summary(2000, 1500, 0),
            eur(800),
            eur(1000),
            eur(0),
            eur(22_950),
        );
        assert_eq!(s.amount, eur(0));
        assert_eq!(s.above_cushion, eur(0));
    }

    #[test]
    fn le_plafond_du_livret_borne_le_versement() {
        let s = savings_suggestion(
            &summary(2000, 1500, 0),
            eur(5000),
            eur(1000),
            eur(22_800),
            eur(22_950),
        );
        assert_eq!(s.amount, eur(150));
        assert_eq!(s.limited_by, SavingsLimit::Ceiling);
        // Livret plein ou au-dessus du plafond : rien, jamais de négatif.
        let full = savings_suggestion(
            &summary(2000, 1500, 0),
            eur(5000),
            eur(1000),
            eur(23_000),
            eur(22_950),
        );
        assert_eq!(full.amount, eur(0));
        assert_eq!(full.room, eur(0));
    }

    #[test]
    fn un_mois_deficitaire_ne_suggere_rien() {
        let s = savings_suggestion(
            &summary(1000, 1500, 0),
            eur(5000),
            eur(1000),
            eur(0),
            eur(22_950),
        );
        assert_eq!(s.amount, eur(0));
        assert_eq!(s.surplus, eur(0));
    }

    #[test]
    fn ce_qui_est_deja_vire_ce_mois_est_deduit() {
        // 500 € de reste, dont 200 € déjà virés : il reste 300 € à verser.
        let s = savings_suggestion(
            &summary(2000, 1500, 200),
            eur(5000),
            eur(1000),
            eur(0),
            eur(22_950),
        );
        assert_eq!(s.amount, eur(300));
        assert_eq!(s.already_saved, eur(200));
        // Tout est déjà viré : rien de plus.
        let done = savings_suggestion(
            &summary(2000, 1500, 500),
            eur(5000),
            eur(1000),
            eur(0),
            eur(22_950),
        );
        assert_eq!(done.amount, eur(0));
    }
}
