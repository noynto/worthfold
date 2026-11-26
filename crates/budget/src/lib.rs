//! Domaine du budget : un plafond mensuel par catégorie de dépense (les enveloppes),
//! et le réglage de l'épargne (où verser ce qui reste en fin de mois).

use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use shared::{AssetId, CategoryId, Currency, EnvelopeId, Money};

/// Le premier jour du mois d'une date : un mois se désigne par ce jour-là.
pub fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1)
        .expect("le premier jour existe dans tous les mois")
}

/// Le premier jour du mois suivant.
pub fn next_month(month: NaiveDate) -> NaiveDate {
    let month = month_start(month);
    if month.month() == 12 {
        NaiveDate::from_ymd_opt(month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1)
    }
    .expect("le mois suivant existe")
}

/// Le premier jour du mois précédent.
pub fn previous_month(month: NaiveDate) -> NaiveDate {
    let month = month_start(month);
    if month.month() == 1 {
        NaiveDate::from_ymd_opt(month.year() - 1, 12, 1)
    } else {
        NaiveDate::from_ymd_opt(month.year(), month.month() - 1, 1)
    }
    .expect("le mois précédent existe")
}

/// Un plafond de dépenses mensuel pour une catégorie, à partir d'un mois.
///
/// Changer un plafond **ajoute** une ligne (règle 5 : l'historique ne s'écrase jamais) : le plafond
/// d'un mois est celui de la ligne la plus récente qui commence au plus tard ce mois-là. Les mois
/// passés gardent donc le plafond qu'ils avaient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub id: EnvelopeId,
    pub category_id: CategoryId,
    /// Le plafond mensuel, ou `None` : plus d'enveloppe pour cette catégorie à partir de ce mois.
    pub limit: Option<Money>,
    /// Le premier jour du mois où cette ligne commence à s'appliquer.
    pub from_month: NaiveDate,
}

/// Le plafond d'une catégorie pour un mois, ou `None` si elle n'a pas (ou plus) d'enveloppe.
pub fn effective_limit(
    envelopes: &[Envelope],
    category_id: &CategoryId,
    month: NaiveDate,
) -> Option<Money> {
    let month = month_start(month);
    envelopes
        .iter()
        .filter(|e| &e.category_id == category_id && e.from_month <= month)
        .max_by(|a, b| (a.from_month, &a.id).cmp(&(b.from_month, &b.id)))
        .and_then(|e| e.limit)
}

/// Le réglage de l'épargne : où verser ce qui reste en fin de mois.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavingsPlan {
    /// Le livret de destination (un actif de type livret d'épargne).
    pub destination: AssetId,
    /// Le plafond de dépôt du livret : au-delà, on ne propose plus de versement.
    pub ceiling: Money,
    /// Un matelas fixé à la main. Absent, il est évalué d'après les dépenses réelles.
    pub cushion_override: Option<Money>,
}

impl SavingsPlan {
    /// Le plafond du Livret A pour un particulier (à vérifier : il peut changer).
    pub fn default_ceiling(currency: Currency) -> Money {
        Money::new(Decimal::from(22_950), currency)
    }
}

#[async_trait]
pub trait BudgetProvider: Send + Sync {
    /// Toutes les lignes d'enveloppes, de la plus ancienne à la plus récente saisie.
    async fn read_envelopes(&self) -> anyhow::Result<Vec<Envelope>>;
    async fn write_envelope(&self, envelope: &Envelope) -> anyhow::Result<()>;
    /// Le réglage de l'épargne, absent tant qu'il n'a pas été fait.
    async fn read_savings_plan(&self) -> anyhow::Result<Option<SavingsPlan>>;
    /// Enregistre le réglage : il remplace le précédent (c'est un réglage, pas un historique).
    async fn write_savings_plan(&self, plan: &SavingsPlan) -> anyhow::Result<()>;
    /// Supprime les enveloppes d'une catégorie (avant de supprimer la catégorie).
    async fn delete_envelopes_of_category(&self, category_id: &CategoryId) -> anyhow::Result<()>;
    /// Supprime le réglage de l'épargne (quand son livret de destination est supprimé).
    async fn delete_savings_plan(&self) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn envelope(category: &str, limit: Option<i64>, from: NaiveDate) -> Envelope {
        Envelope {
            id: EnvelopeId::generate(),
            category_id: category.into(),
            limit: limit.map(|l| Money::new(Decimal::from(l), Currency::Eur)),
            from_month: from,
        }
    }

    fn eur(amount: i64) -> Option<Money> {
        Some(Money::new(Decimal::from(amount), Currency::Eur))
    }

    #[test]
    fn navigue_entre_les_mois_y_compris_au_changement_d_annee() {
        assert_eq!(month_start(day(2026, 11, 25)), day(2026, 11, 1));
        assert_eq!(next_month(day(2026, 11, 25)), day(2026, 12, 1));
        assert_eq!(next_month(day(2026, 12, 1)), day(2027, 1, 1));
        assert_eq!(previous_month(day(2026, 11, 9)), day(2026, 10, 1));
        assert_eq!(previous_month(day(2027, 1, 1)), day(2026, 12, 1));
    }

    #[test]
    fn le_plafond_d_un_mois_est_celui_de_la_ligne_la_plus_recente_qui_a_commence() {
        let envelopes = [
            envelope("courses", Some(400), day(2026, 1, 1)),
            envelope("courses", Some(500), day(2026, 6, 1)),
            envelope("autre", Some(10), day(2026, 1, 1)),
        ];
        let category: CategoryId = "courses".into();
        // Avant la première ligne : pas d'enveloppe.
        assert_eq!(
            effective_limit(&envelopes, &category, day(2025, 12, 15)),
            None
        );
        assert_eq!(
            effective_limit(&envelopes, &category, day(2026, 5, 31)),
            eur(400)
        );
        // Un mois passé garde son plafond d'alors, même après une hausse.
        assert_eq!(
            effective_limit(&envelopes, &category, day(2026, 6, 1)),
            eur(500)
        );
        assert_eq!(
            effective_limit(&envelopes, &category, day(2026, 11, 9)),
            eur(500)
        );
    }

    #[test]
    fn une_ligne_sans_plafond_met_fin_a_l_enveloppe() {
        let envelopes = [
            envelope("courses", Some(400), day(2026, 1, 1)),
            envelope("courses", None, day(2026, 9, 1)),
        ];
        let category: CategoryId = "courses".into();
        assert_eq!(
            effective_limit(&envelopes, &category, day(2026, 8, 1)),
            eur(400)
        );
        assert_eq!(
            effective_limit(&envelopes, &category, day(2026, 9, 1)),
            None
        );
    }

    #[test]
    fn deux_lignes_du_meme_mois_la_derniere_saisie_l_emporte() {
        let first = envelope("courses", Some(400), day(2026, 6, 1));
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = envelope("courses", Some(450), day(2026, 6, 1));
        let category: CategoryId = "courses".into();
        assert_eq!(
            effective_limit(
                &[second.clone(), first.clone()],
                &category,
                day(2026, 6, 20)
            ),
            eur(450)
        );
        assert_eq!(
            effective_limit(&[first, second], &category, day(2026, 6, 20)),
            eur(450)
        );
    }
}
