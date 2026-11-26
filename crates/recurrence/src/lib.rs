//! Domaine des paiements prévus : ce qui revient ou arrive à date connue (salaire, fermage, impôts, abonnements).
//!
//! Un paiement se décrit en trois blocs : **quand** (`Schedule`), **combien** (`Amount`) et
//! **pendant combien de temps** (`Window`). Une fonction pure, `occurrences`, les déplie en échéances datées.

use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use rust_decimal::{Decimal, RoundingStrategy};
use shared::{AssetId, CategoryId, Money, PlannedPaymentId};

/// Le premier jour du mois d'une date.
fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1)
        .expect("le premier jour existe dans tous les mois")
}

fn next_month(month: NaiveDate) -> NaiveDate {
    let month = month_start(month);
    if month.month() == 12 {
        NaiveDate::from_ymd_opt(month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1)
    }
    .expect("le mois suivant existe")
}

fn last_day_of_month(month: NaiveDate) -> u32 {
    next_month(month)
        .pred_opt()
        .expect("le jour précédent existe")
        .day()
}

/// Le nombre de mois entre deux mois (négatif si `to` précède `from`).
fn months_between(from: NaiveDate, to: NaiveDate) -> i32 {
    (to.year() - from.year()) * 12 + to.month() as i32 - from.month() as i32
}

/// Dans quel sens va l'argent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Une entrée d'argent (salaire, fermage).
    In,
    /// Une sortie d'argent (loyer, impôt, abonnement).
    Out,
}

impl Direction {
    pub fn code(self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "in" => Some(Direction::In),
            "out" => Some(Direction::Out),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Direction::In => "Entrée d'argent",
            Direction::Out => "Sortie d'argent",
        }
    }

    /// Un montant s'enregistre en positif ; le sens donne son signe dans les comptes.
    pub fn signed(self, amount: Decimal) -> Decimal {
        match self {
            Direction::In => amount,
            Direction::Out => -amount,
        }
    }
}

/// Le jour du mois où tombe l'échéance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayOfMonth {
    /// Le jour donné (1 à 31) ; dans un mois plus court, le dernier jour du mois : « le 31 » donne le 28 février.
    Day(u32),
    /// Le dernier jour du mois.
    Last,
}

impl DayOfMonth {
    pub fn validate(self) -> Result<Self, String> {
        match self {
            DayOfMonth::Day(d) if !(1..=31).contains(&d) => {
                Err("Le jour du mois doit être compris entre 1 et 31.".into())
            }
            day => Ok(day),
        }
    }

    /// La date dans le mois donné.
    fn in_month(self, month: NaiveDate) -> NaiveDate {
        let last = last_day_of_month(month);
        let day = match self {
            DayOfMonth::Day(d) => d.min(last),
            DayOfMonth::Last => last,
        };
        month
            .with_day(day)
            .expect("le jour est borné par la fin du mois")
    }
}

/// Quand tombe le paiement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schedule {
    /// Chaque mois.
    Monthly { day: DayOfMonth },
    /// Tous les `n` mois (2 : un mois sur deux ; 3 : trimestriel ; 12 : annuel), à partir de `first_month`.
    EveryNMonths {
        n: u32,
        day: DayOfMonth,
        /// Le premier jour du premier mois concerné : il fixe le rythme.
        first_month: NaiveDate,
    },
    /// Seulement certains mois de l'année (de 1 à 12), chaque année : un complément d'impôt d'octobre à décembre.
    OnMonths { months: Vec<u32>, day: DayOfMonth },
    /// Une seule fois.
    Once { date: NaiveDate },
}

impl Schedule {
    pub fn validate(self) -> Result<Self, String> {
        match self {
            Schedule::Monthly { day } => Ok(Schedule::Monthly {
                day: day.validate()?,
            }),
            Schedule::EveryNMonths {
                n,
                day,
                first_month,
            } => {
                if !(2..=24).contains(&n) {
                    return Err(
                        "Le rythme doit être de 2 à 24 mois (pour chaque mois, choisis « Chaque mois »).".into(),
                    );
                }
                Ok(Schedule::EveryNMonths {
                    n,
                    day: day.validate()?,
                    first_month: month_start(first_month),
                })
            }
            Schedule::OnMonths { mut months, day } => {
                months.sort_unstable();
                months.dedup();
                if months.is_empty() || months.iter().any(|m| !(1..=12).contains(m)) {
                    return Err("Choisis au moins un mois, de janvier à décembre.".into());
                }
                Ok(Schedule::OnMonths {
                    months,
                    day: day.validate()?,
                })
            }
            once @ Schedule::Once { .. } => Ok(once),
        }
    }

    /// Les dates du rythme dans le mois, avant toute autre règle (fenêtre, exceptions).
    fn date_in_month(&self, month: NaiveDate) -> Option<NaiveDate> {
        match self {
            Schedule::Monthly { day } => Some(day.in_month(month)),
            Schedule::EveryNMonths {
                n,
                day,
                first_month,
            } => {
                let gap = months_between(*first_month, month);
                (gap >= 0 && gap % *n as i32 == 0).then(|| day.in_month(month))
            }
            Schedule::OnMonths { months, day } => {
                months.contains(&month.month()).then(|| day.in_month(month))
            }
            Schedule::Once { date } => (month_start(*date) == month_start(month)).then_some(*date),
        }
    }
}

/// Un montant à partir d'une date. Une marge nulle dit un montant exact ; sinon c'est une estimation « à ± la marge ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmountStep {
    pub from: NaiveDate,
    /// Positif : le sens du paiement lui donne son signe.
    pub value: Money,
    pub margin: Money,
}

/// La révision annuelle attendue (indice des fermages, taux d'imposition…), tant que le vrai montant n'est pas connu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revision {
    /// Le mois (1 à 12) et le jour de la révision, chaque année.
    pub month: u32,
    pub day: u32,
    /// La variation estimée, en pourcentage : `2` pour +2 %, `-1.5` pour −1,5 %.
    pub change: Decimal,
}

impl Revision {
    fn date_in_year(&self, year: i32) -> NaiveDate {
        let month = NaiveDate::from_ymd_opt(year, self.month, 1).expect("mois validé");
        month
            .with_day(self.day.min(last_day_of_month(month)))
            .expect("jour borné")
    }

    pub fn validate(self) -> Result<Self, String> {
        if !(1..=12).contains(&self.month) || !(1..=31).contains(&self.day) {
            return Err("La date de révision n'est pas valide.".into());
        }
        if self.change <= Decimal::from(-100) || self.change > Decimal::from(100) {
            return Err("La variation estimée doit être comprise entre −100 % et 100 %.".into());
        }
        Ok(self)
    }
}

/// Une valeur et une marge observées sur les paiements réels (voir `Amount::observed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observed {
    pub value: Money,
    pub margin: Money,
}

/// Combien.
///
/// Les montants vont **par paliers** : le premier vaut dès le début, et chaque changement connu **ajoute** un palier
/// (l'historique ne s'écrase jamais). Entre deux paliers, le dernier connu s'applique ; la révision annuelle
/// estimée ne sert qu'à prévoir ce qu'on ne connaît pas encore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount {
    /// Au moins un palier, du plus ancien au plus récent.
    pub steps: Vec<AmountStep>,
    pub revision: Option<Revision>,
    /// La valeur vient des paiements réels (médiane des derniers versements) quand il y en a :
    /// pour un salaire qui varie. Le premier palier sert alors tant qu'il n'y a pas d'historique.
    pub observed: bool,
}

/// Ce qu'on attend d'un paiement à une date : valeur, marge, et si c'est une estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected {
    pub value: Money,
    pub margin: Money,
    pub estimated: bool,
}

impl Amount {
    pub fn validate(mut self) -> Result<Self, String> {
        if self.steps.is_empty() {
            return Err("Le montant est nécessaire.".into());
        }
        self.steps.sort_by_key(|s| s.from);
        for step in &self.steps {
            if step.value.amount.is_sign_negative() && !step.value.amount.is_zero() {
                return Err(
                    "Saisis le montant en positif : le sens du paiement donne son signe.".into(),
                );
            }
            if step.margin.amount.is_sign_negative() && !step.margin.amount.is_zero() {
                return Err("La marge ne peut pas être négative.".into());
            }
            if step.margin.currency != step.value.currency {
                return Err("Le montant et sa marge doivent avoir la même devise.".into());
            }
        }
        self.revision = self.revision.map(Revision::validate).transpose()?;
        Ok(self)
    }

    /// Le palier qui s'applique à une date : le plus récent commencé, sinon le premier.
    fn step_at(&self, date: NaiveDate) -> &AmountStep {
        self.steps
            .iter()
            .rfind(|s| s.from <= date)
            .unwrap_or(&self.steps[0])
    }

    /// Ce qu'on attend à une date. `observed` : les paiements réels, s'il y en a.
    pub fn expected_at(&self, date: NaiveDate, observed: Option<Observed>) -> Expected {
        if self.observed
            && let Some(observed) = observed
        {
            return Expected {
                value: observed.value,
                margin: observed.margin,
                estimated: true,
            };
        }
        let step = self.step_at(date);
        // Chaque révision annuelle passée depuis le palier sans vrai montant connu s'applique une fois.
        let revisions = self.revision.map_or(0, |r| {
            (step.from.year()..=date.year())
                .map(|year| r.date_in_year(year))
                .filter(|d| *d > step.from && *d <= date)
                .count()
        });
        let mut value = step.value.amount;
        if let Some(revision) = self.revision {
            let factor = Decimal::ONE + revision.change / Decimal::ONE_HUNDRED;
            for _ in 0..revisions {
                value *= factor;
            }
        }
        let value = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
        let drift = (value - step.value.amount).abs();
        Expected {
            value: Money::new(value, step.value.currency),
            margin: Money::new(step.margin.amount + drift, step.margin.currency),
            estimated: revisions > 0 || !step.margin.amount.is_zero(),
        }
    }
}

/// Pendant combien de temps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Window {
    pub from: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    /// Le nombre total d'échéances (un paiement en 4 fois). Demande une date de début.
    pub occurrences: Option<u32>,
}

impl Window {
    pub fn validate(self) -> Result<Self, String> {
        if let (Some(from), Some(until)) = (self.from, self.until)
            && until < from
        {
            return Err("La date de fin précède la date de début.".into());
        }
        if self.occurrences == Some(0) {
            return Err("Le nombre d'échéances doit être d'au moins une.".into());
        }
        if self.occurrences.is_some() && self.from.is_none() {
            return Err("Pour limiter le nombre d'échéances, indique la date de début.".into());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPayment {
    pub id: PlannedPaymentId,
    pub label: String,
    /// Le mot du libellé bancaire qui reconnaît ce paiement dans les relevés (« salaire », « edf »).
    pub pattern: String,
    pub direction: Direction,
    /// Le compte bancaire concerné.
    pub account_id: AssetId,
    pub category_id: Option<CategoryId>,
    pub schedule: Schedule,
    pub amount: Amount,
    pub window: Window,
}

impl PlannedPayment {
    pub const LABEL_MAX_LENGTH: usize = 100;

    pub fn validate_label(label: &str) -> Result<String, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("Le nom du paiement est nécessaire.".into());
        }
        if label.chars().count() > Self::LABEL_MAX_LENGTH {
            return Err(format!(
                "Le nom du paiement ne doit pas dépasser {} caractères.",
                Self::LABEL_MAX_LENGTH
            ));
        }
        Ok(label.to_owned())
    }

    pub fn validate_pattern(pattern: &str) -> Result<String, String> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return Err("Le mot du libellé bancaire est nécessaire pour reconnaître ce paiement : par exemple « salaire ».".into());
        }
        if pattern.chars().count() > Self::LABEL_MAX_LENGTH {
            return Err(format!(
                "Le mot du libellé ne doit pas dépasser {} caractères.",
                Self::LABEL_MAX_LENGTH
            ));
        }
        Ok(pattern.to_owned())
    }
}

/// Ce que l'on change pour une échéance précise, sans toucher au paiement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionAction {
    /// Pas d'échéance ce jour-là (un mois de vacances sans prélèvement).
    Skip,
    /// Un autre montant ce jour-là, exact, en positif.
    Amount(Money),
}

/// Une échéance d'un paiement : une date, un montant signé, et ce qui la distingue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub payment_id: PlannedPaymentId,
    pub date: NaiveDate,
    /// Signé : négatif pour une sortie d'argent.
    pub amount: Money,
    /// Incertitude autour du montant (≥ 0).
    pub margin: Money,
    /// Le montant est une estimation (marge, révision attendue ou valeur observée).
    pub estimated: bool,
    /// Sautée par une exception : elle ne compte pas dans le prévisionnel.
    pub skipped: bool,
    /// Le montant a été changé par une exception.
    pub overridden: bool,
}

/// Déplie un paiement en échéances entre deux dates (incluses).
///
/// `exceptions` ne contient que celles du paiement. `observed` : les valeurs tirées des paiements réels,
/// pour un montant « observé ». Une fonction pure : ni base ni horloge.
pub fn occurrences(
    payment: &PlannedPayment,
    exceptions: &[(NaiveDate, ExceptionAction)],
    observed: Option<Observed>,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<Occurrence> {
    if to < from {
        return Vec::new();
    }
    // Limiter le nombre d'échéances oblige à les compter depuis le début de la fenêtre.
    let scan_from = match (payment.window.occurrences, payment.window.from) {
        (Some(_), Some(window_from)) => window_from.min(from),
        _ => from,
    };
    let mut month = month_start(scan_from);
    let last_month = month_start(to);
    let mut result = Vec::new();
    let mut counted: u32 = 0;
    while month <= last_month {
        if let Some(date) = payment.schedule.date_in_month(month) {
            let in_window = payment.window.from.is_none_or(|f| date >= f)
                && payment.window.until.is_none_or(|u| date <= u);
            if in_window {
                counted += 1;
                if payment.window.occurrences.is_some_and(|max| counted > max) {
                    break;
                }
                if date >= from && date <= to {
                    result.push(build(payment, exceptions, observed, date));
                }
            }
        }
        month = next_month(month);
    }
    result
}

fn build(
    payment: &PlannedPayment,
    exceptions: &[(NaiveDate, ExceptionAction)],
    observed: Option<Observed>,
    date: NaiveDate,
) -> Occurrence {
    let expected = payment.amount.expected_at(date, observed);
    let exception = exceptions.iter().find(|(d, _)| *d == date).map(|(_, a)| *a);
    let (value, margin, estimated, skipped, overridden) = match exception {
        Some(ExceptionAction::Skip) => (
            expected.value,
            expected.margin,
            expected.estimated,
            true,
            false,
        ),
        Some(ExceptionAction::Amount(m)) => (m, Money::zero(m.currency), false, false, true),
        None => (
            expected.value,
            expected.margin,
            expected.estimated,
            false,
            false,
        ),
    };
    Occurrence {
        payment_id: payment.id.clone(),
        date,
        amount: Money::new(payment.direction.signed(value.amount), value.currency),
        margin,
        estimated,
        skipped,
        overridden,
    }
}

#[async_trait]
pub trait RecurrenceProvider: Send + Sync {
    /// Les paiements prévus, du plus ancien au plus récent.
    async fn read_all(&self) -> anyhow::Result<Vec<PlannedPayment>>;
    async fn read(&self, id: &PlannedPaymentId) -> anyhow::Result<Option<PlannedPayment>>;
    /// Crée le paiement, ou le remplace s'il existe (ajout d'un palier de montant).
    async fn write(&self, payment: &PlannedPayment) -> anyhow::Result<()>;
    /// Supprime le paiement et ses exceptions.
    async fn delete(&self, id: &PlannedPaymentId) -> anyhow::Result<()>;
    /// Les exceptions de tous les paiements : (paiement, date de l'échéance, action).
    async fn read_exceptions(
        &self,
    ) -> anyhow::Result<Vec<(PlannedPaymentId, NaiveDate, ExceptionAction)>>;
    /// Pose l'exception, ou remplace celle de la même échéance.
    async fn write_exception(
        &self,
        payment_id: &PlannedPaymentId,
        date: NaiveDate,
        action: ExceptionAction,
    ) -> anyhow::Result<()>;
    async fn delete_exception(
        &self,
        payment_id: &PlannedPaymentId,
        date: NaiveDate,
    ) -> anyhow::Result<()>;
    /// Détache les paiements d'une catégorie qui va être supprimée.
    async fn clear_category(&self, category_id: &CategoryId) -> anyhow::Result<()>;
    /// Supprime les paiements d'un compte qui va être supprimé, avec leurs exceptions.
    async fn delete_of_account(&self, account_id: &AssetId) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::Currency;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn step(from: NaiveDate, value: i64, margin: i64) -> AmountStep {
        AmountStep {
            from,
            value: eur(value),
            margin: eur(margin),
        }
    }

    fn payment(schedule: Schedule, amount: Amount, window: Window) -> PlannedPayment {
        PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: "Paiement".into(),
            pattern: "paiement".into(),
            direction: Direction::Out,
            account_id: "compte".into(),
            category_id: None,
            schedule,
            amount,
            window,
        }
    }

    fn fixed(value: i64) -> Amount {
        Amount {
            steps: vec![step(day(2026, 1, 1), value, 0)],
            revision: None,
            observed: false,
        }
    }

    fn dates(payment: &PlannedPayment, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        occurrences(payment, &[], None, from, to)
            .into_iter()
            .map(|o| o.date)
            .collect()
    }

    // ── Rythmes ──

    #[test]
    fn un_paiement_mensuel_tombe_chaque_mois_et_le_31_devient_la_fin_du_mois() {
        let p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Day(31),
            },
            fixed(10),
            Window::default(),
        );
        assert_eq!(
            dates(&p, day(2027, 1, 1), day(2027, 4, 30)),
            [
                day(2027, 1, 31),
                day(2027, 2, 28),
                day(2027, 3, 31),
                day(2027, 4, 30)
            ]
        );
        // Une année bissextile garde le 29.
        assert_eq!(
            dates(&p, day(2028, 2, 1), day(2028, 2, 29)),
            [day(2028, 2, 29)]
        );
    }

    #[test]
    fn le_dernier_jour_du_mois() {
        let p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Last,
            },
            fixed(10),
            Window::default(),
        );
        assert_eq!(
            dates(&p, day(2026, 11, 1), day(2026, 12, 31)),
            [day(2026, 11, 30), day(2026, 12, 31)]
        );
    }

    #[test]
    fn un_rythme_trimestriel_suit_le_premier_mois_et_ignore_ceux_d_avant() {
        let p = payment(
            Schedule::EveryNMonths {
                n: 3,
                day: DayOfMonth::Day(15),
                first_month: day(2026, 2, 1),
            },
            fixed(10),
            Window::default(),
        );
        assert_eq!(
            dates(&p, day(2025, 11, 1), day(2027, 3, 31)),
            [
                day(2026, 2, 15),
                day(2026, 5, 15),
                day(2026, 8, 15),
                day(2026, 11, 15),
                day(2027, 2, 15)
            ]
        );
    }

    #[test]
    fn des_mois_choisis_reviennent_chaque_annee() {
        let p = payment(
            Schedule::OnMonths {
                months: vec![10, 11, 12],
                day: DayOfMonth::Day(15),
            },
            fixed(10),
            Window::default(),
        );
        assert_eq!(
            dates(&p, day(2026, 9, 1), day(2027, 10, 31)),
            [
                day(2026, 10, 15),
                day(2026, 11, 15),
                day(2026, 12, 15),
                day(2027, 10, 15)
            ]
        );
    }

    #[test]
    fn une_echeance_unique_n_est_dans_la_periode_que_si_sa_date_y_est() {
        let p = payment(
            Schedule::Once {
                date: day(2026, 12, 20),
            },
            fixed(10),
            Window::default(),
        );
        assert_eq!(
            dates(&p, day(2026, 12, 1), day(2026, 12, 31)),
            [day(2026, 12, 20)]
        );
        assert!(dates(&p, day(2026, 12, 21), day(2027, 12, 31)).is_empty());
        assert!(dates(&p, day(2026, 1, 1), day(2026, 12, 19)).is_empty());
    }

    #[test]
    fn une_periode_vide_ou_inversee_ne_donne_rien() {
        let p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            fixed(10),
            Window::default(),
        );
        assert!(dates(&p, day(2026, 12, 1), day(2026, 11, 1)).is_empty());
        assert!(dates(&p, day(2026, 11, 6), day(2026, 11, 30)).is_empty());
    }

    // ── Fenêtre ──

    #[test]
    fn la_fenetre_borne_le_debut_et_la_fin() {
        let p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            fixed(10),
            Window {
                from: Some(day(2026, 3, 1)),
                until: Some(day(2026, 5, 31)),
                occurrences: None,
            },
        );
        assert_eq!(
            dates(&p, day(2026, 1, 1), day(2026, 12, 31)),
            [day(2026, 3, 5), day(2026, 4, 5), day(2026, 5, 5)]
        );
    }

    #[test]
    fn un_paiement_en_quatre_fois_s_arrete_apres_la_quatrieme() {
        let p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Day(10),
            },
            fixed(10),
            Window {
                from: Some(day(2026, 9, 1)),
                until: None,
                occurrences: Some(4),
            },
        );
        assert_eq!(
            dates(&p, day(2026, 1, 1), day(2027, 12, 31)),
            [
                day(2026, 9, 10),
                day(2026, 10, 10),
                day(2026, 11, 10),
                day(2026, 12, 10)
            ]
        );
        // Regarder plus tard ne change pas le compte : les échéances se comptent depuis le début.
        assert_eq!(
            dates(&p, day(2026, 11, 1), day(2027, 12, 31)),
            [day(2026, 11, 10), day(2026, 12, 10)]
        );
        assert!(dates(&p, day(2027, 1, 1), day(2027, 12, 31)).is_empty());
    }

    // ── Montants ──

    #[test]
    fn le_dernier_palier_commence_s_applique_et_les_mois_passes_gardent_le_leur() {
        let amount = Amount {
            steps: vec![step(day(2026, 1, 1), 700, 0), step(day(2026, 7, 1), 720, 0)],
            revision: None,
            observed: false,
        };
        assert_eq!(amount.expected_at(day(2026, 6, 30), None).value, eur(700));
        assert_eq!(amount.expected_at(day(2026, 7, 1), None).value, eur(720));
        assert_eq!(amount.expected_at(day(2030, 1, 1), None).value, eur(720));
        // Avant le premier palier, le premier s'applique.
        assert_eq!(amount.expected_at(day(2025, 1, 1), None).value, eur(700));
        assert!(!amount.expected_at(day(2026, 7, 1), None).estimated);
    }

    #[test]
    fn la_revision_annuelle_estimee_s_applique_une_fois_par_an_sans_vrai_montant() {
        let amount = Amount {
            steps: vec![step(day(2026, 1, 1), 1000, 0)],
            revision: Some(Revision {
                month: 10,
                day: 15,
                change: Decimal::from(2),
            }),
            observed: false,
        };
        let at = |y, m, d| amount.expected_at(day(y, m, d), None);
        // Avant la première révision : le montant connu.
        assert_eq!(at(2026, 10, 14).value, eur(1000));
        assert!(!at(2026, 10, 14).estimated);
        // Après : +2 %, estimé, avec une marge égale à l'écart.
        let after = at(2026, 10, 15);
        assert_eq!(
            after.value,
            Money::new(Decimal::new(102000, 2), Currency::Eur)
        );
        assert_eq!(after.margin, Money::new(Decimal::from(20), Currency::Eur));
        assert!(after.estimated);
        // Deux révisions sans vrai montant : +2 % deux fois.
        assert_eq!(
            at(2027, 11, 1).value,
            Money::new(Decimal::new(104040, 2), Currency::Eur)
        );
    }

    #[test]
    fn le_vrai_montant_connu_remplace_l_estimation_de_la_revision() {
        let amount = Amount {
            steps: vec![
                step(day(2026, 1, 1), 1000, 0),
                step(day(2026, 10, 15), 1015, 0), // l'indice publié : +1,5 %
            ],
            revision: Some(Revision {
                month: 10,
                day: 15,
                change: Decimal::from(2),
            }),
            observed: false,
        };
        let expected = amount.expected_at(day(2026, 12, 1), None);
        assert_eq!(expected.value, eur(1015));
        assert!(!expected.estimated);
        // L'année suivante, la révision estimée repart du dernier vrai montant.
        let next = amount.expected_at(day(2027, 11, 1), None);
        assert_eq!(
            next.value,
            Money::new(Decimal::new(103530, 2), Currency::Eur)
        );
        assert!(next.estimated);
    }

    #[test]
    fn une_estimation_garde_sa_marge_et_une_valeur_observee_prend_le_dessus() {
        let amount = Amount {
            steps: vec![step(day(2026, 1, 1), 1800, 150)],
            revision: None,
            observed: true,
        };
        let estimated = amount.expected_at(day(2026, 11, 1), None);
        assert_eq!(
            (estimated.value, estimated.margin, estimated.estimated),
            (eur(1800), eur(150), true)
        );

        let observed = Observed {
            value: eur(1900),
            margin: eur(80),
        };
        let from_history = amount.expected_at(day(2026, 11, 1), Some(observed));
        assert_eq!(
            (from_history.value, from_history.margin),
            (eur(1900), eur(80))
        );

        // Un montant non « observé » ignore l'historique.
        let plain = Amount {
            observed: false,
            ..amount
        };
        assert_eq!(
            plain.expected_at(day(2026, 11, 1), Some(observed)).value,
            eur(1800)
        );
    }

    // ── Échéances ──

    #[test]
    fn le_sens_donne_le_signe_de_l_echeance() {
        let mut p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Day(1),
            },
            fixed(700),
            Window::default(),
        );
        let out = occurrences(&p, &[], None, day(2026, 11, 1), day(2026, 11, 30));
        assert_eq!(out[0].amount, eur(-700));
        p.direction = Direction::In;
        let income = occurrences(&p, &[], None, day(2026, 11, 1), day(2026, 11, 30));
        assert_eq!(income[0].amount, eur(700));
    }

    #[test]
    fn une_exception_saute_ou_change_une_echeance_sans_toucher_aux_autres() {
        let p = payment(
            Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            fixed(100),
            Window::default(),
        );
        let exceptions = [
            (day(2026, 11, 5), ExceptionAction::Skip),
            (day(2026, 12, 5), ExceptionAction::Amount(eur(250))),
        ];
        let result = occurrences(&p, &exceptions, None, day(2026, 10, 1), day(2027, 1, 31));
        let summary: Vec<_> = result
            .iter()
            .map(|o| (o.date, o.amount, o.skipped, o.overridden))
            .collect();
        assert_eq!(
            summary,
            [
                (day(2026, 10, 5), eur(-100), false, false),
                (day(2026, 11, 5), eur(-100), true, false),
                (day(2026, 12, 5), eur(-250), false, true),
                (day(2027, 1, 5), eur(-100), false, false),
            ]
        );
        // Un montant changé par exception est exact.
        assert!(!result[2].estimated);
    }

    // ── Validations ──

    #[test]
    fn valide_les_rythmes() {
        assert!(
            Schedule::Monthly {
                day: DayOfMonth::Day(0)
            }
            .validate()
            .is_err()
        );
        assert!(
            Schedule::Monthly {
                day: DayOfMonth::Day(32)
            }
            .validate()
            .is_err()
        );
        for n in [0, 1, 25] {
            assert!(
                Schedule::EveryNMonths {
                    n,
                    day: DayOfMonth::Last,
                    first_month: day(2026, 1, 1)
                }
                .validate()
                .is_err(),
                "n = {n}"
            );
        }
        assert!(
            Schedule::OnMonths {
                months: vec![],
                day: DayOfMonth::Last
            }
            .validate()
            .is_err()
        );
        assert!(
            Schedule::OnMonths {
                months: vec![13],
                day: DayOfMonth::Last
            }
            .validate()
            .is_err()
        );
        // Les mois sont triés et sans doublon ; le premier mois est ramené au 1er.
        assert_eq!(
            Schedule::OnMonths {
                months: vec![12, 10, 10],
                day: DayOfMonth::Last
            }
            .validate(),
            Ok(Schedule::OnMonths {
                months: vec![10, 12],
                day: DayOfMonth::Last
            })
        );
        assert_eq!(
            Schedule::EveryNMonths {
                n: 3,
                day: DayOfMonth::Last,
                first_month: day(2026, 5, 17)
            }
            .validate(),
            Ok(Schedule::EveryNMonths {
                n: 3,
                day: DayOfMonth::Last,
                first_month: day(2026, 5, 1)
            })
        );
    }

    #[test]
    fn valide_les_montants_et_les_fenetres() {
        assert!(
            Amount {
                steps: vec![],
                revision: None,
                observed: false
            }
            .validate()
            .is_err()
        );
        assert!(
            Amount {
                steps: vec![step(day(2026, 1, 1), -5, 0)],
                revision: None,
                observed: false
            }
            .validate()
            .is_err()
        );
        assert!(
            Amount {
                steps: vec![step(day(2026, 1, 1), 5, -1)],
                revision: None,
                observed: false
            }
            .validate()
            .is_err()
        );
        let too_big = Revision {
            month: 10,
            day: 15,
            change: Decimal::from(101),
        };
        assert!(
            Amount {
                steps: vec![step(day(2026, 1, 1), 5, 0)],
                revision: Some(too_big),
                observed: false
            }
            .validate()
            .is_err()
        );
        // Les paliers sont triés par date.
        let sorted = Amount {
            steps: vec![step(day(2026, 7, 1), 2, 0), step(day(2026, 1, 1), 1, 0)],
            revision: None,
            observed: false,
        }
        .validate()
        .unwrap();
        assert_eq!(sorted.steps[0].from, day(2026, 1, 1));

        assert!(
            Window {
                from: Some(day(2026, 5, 1)),
                until: Some(day(2026, 4, 1)),
                occurrences: None
            }
            .validate()
            .is_err()
        );
        assert!(
            Window {
                from: None,
                until: None,
                occurrences: Some(0)
            }
            .validate()
            .is_err()
        );
        assert!(
            Window {
                from: None,
                until: None,
                occurrences: Some(4)
            }
            .validate()
            .is_err()
        );
        assert!(
            Window {
                from: Some(day(2026, 5, 1)),
                until: None,
                occurrences: Some(4)
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn valide_le_nom_et_le_mot_du_libelle() {
        assert_eq!(
            PlannedPayment::validate_label("  Loyer "),
            Ok("Loyer".into())
        );
        assert!(PlannedPayment::validate_label(" ").is_err());
        assert!(PlannedPayment::validate_label(&"x".repeat(101)).is_err());
        assert!(PlannedPayment::validate_pattern("  ").is_err());
        assert_eq!(PlannedPayment::validate_pattern(" edf "), Ok("edf".into()));
    }

    #[test]
    fn le_code_du_sens_fait_l_aller_retour() {
        for direction in [Direction::In, Direction::Out] {
            assert_eq!(Direction::from_code(direction.code()), Some(direction));
        }
        assert_eq!(Direction::from_code("autre"), None);
    }
}
