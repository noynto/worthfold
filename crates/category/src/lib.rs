//! Domaine des catégories : ranger les transactions (courses, loyer, salaire…), à la main
//! ou par des règles sur le libellé.

use async_trait::async_trait;
use shared::{CategoryId, CategoryRuleId};

/// Le sens d'une catégorie : il décide si une opération compte dans les dépenses, les revenus, ou aucun des deux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CategoryKind {
    Expense,
    Income,
    /// Un virement entre ses propres comptes : neutre, sinon il gonflerait à la fois revenus et dépenses.
    Transfer,
}

impl CategoryKind {
    pub const ALL: [CategoryKind; 3] = [
        CategoryKind::Expense,
        CategoryKind::Income,
        CategoryKind::Transfer,
    ];

    /// Code stable, enregistré en base et envoyé par les formulaires.
    pub fn code(self) -> &'static str {
        match self {
            CategoryKind::Expense => "expense",
            CategoryKind::Income => "income",
            CategoryKind::Transfer => "transfer",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            CategoryKind::Expense => "Dépense",
            CategoryKind::Income => "Revenu",
            CategoryKind::Transfer => "Virement interne",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub kind: CategoryKind,
}

impl Category {
    pub const NAME_MAX_LENGTH: usize = 60;

    /// Les catégories usuelles proposées au premier lancement (jamais créées sans que l'on le demande).
    pub const DEFAULTS: [(&'static str, CategoryKind); 14] = [
        ("Courses", CategoryKind::Expense),
        ("Logement", CategoryKind::Expense),
        ("Énergie et eau", CategoryKind::Expense),
        ("Transports", CategoryKind::Expense),
        ("Santé", CategoryKind::Expense),
        ("Restaurants et sorties", CategoryKind::Expense),
        ("Loisirs", CategoryKind::Expense),
        ("Abonnements", CategoryKind::Expense),
        ("Impôts et taxes", CategoryKind::Expense),
        ("Assurances", CategoryKind::Expense),
        ("Salaire", CategoryKind::Income),
        ("Fermages et loyers perçus", CategoryKind::Income),
        ("Autres revenus", CategoryKind::Income),
        ("Virement interne", CategoryKind::Transfer),
    ];

    /// Le nom nettoyé, ou le refus en français.
    pub fn validate_name(name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Le nom de la catégorie est nécessaire.".into());
        }
        if name.chars().count() > Self::NAME_MAX_LENGTH {
            return Err(format!(
                "Le nom de la catégorie ne doit pas dépasser {} caractères.",
                Self::NAME_MAX_LENGTH
            ));
        }
        Ok(name.to_owned())
    }

    /// Deux catégories ne portent pas le même nom (sans tenir compte de la casse ni des accents).
    pub fn same_name(&self, other: &str) -> bool {
        normalize(&self.name) == normalize(other)
    }
}

/// Une règle d'affectation : tout libellé qui **contient** le motif reçoit la catégorie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryRule {
    pub id: CategoryRuleId,
    pub category_id: CategoryId,
    pub pattern: String,
}

impl CategoryRule {
    pub const PATTERN_MAX_LENGTH: usize = 100;

    /// Le motif nettoyé, ou le refus en français.
    pub fn validate_pattern(pattern: &str) -> Result<String, String> {
        let pattern = pattern.trim();
        if normalize(pattern).is_empty() {
            return Err(
                "Le motif de la règle est nécessaire : un mot du libellé, comme « carrefour »."
                    .into(),
            );
        }
        if pattern.chars().count() > Self::PATTERN_MAX_LENGTH {
            return Err(format!(
                "Le motif ne doit pas dépasser {} caractères.",
                Self::PATTERN_MAX_LENGTH
            ));
        }
        Ok(pattern.to_owned())
    }

    /// Le libellé contient le motif, sans tenir compte de la casse, des accents ni des espaces multiples.
    pub fn matches(&self, label: &str) -> bool {
        normalize(label).contains(&normalize(&self.pattern))
    }
}

/// La catégorie d'un libellé selon les règles, ou `None`.
/// Quand plusieurs règles conviennent, la plus **précise** (le motif le plus long) l'emporte :
/// « carrefour city » passe avant « carrefour ». À précision égale, la plus ancienne gagne.
pub fn matching_category(label: &str, rules: &[CategoryRule]) -> Option<CategoryId> {
    rules
        .iter()
        .filter(|rule| rule.matches(label))
        .min_by(|a, b| {
            let (la, lb) = (normalize(&a.pattern).len(), normalize(&b.pattern).len());
            lb.cmp(&la).then_with(|| a.id.cmp(&b.id))
        })
        .map(|rule| rule.category_id.clone())
}

/// Minuscules, sans accents, espaces réduits à un seul : pour comparer des libellés bancaires.
pub fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' | 'ä' => 'a',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
pub trait CategoryProvider: Send + Sync {
    /// Les catégories, par nom.
    async fn read_all(&self) -> anyhow::Result<Vec<Category>>;
    async fn read(&self, id: &CategoryId) -> anyhow::Result<Option<Category>>;
    async fn write(&self, category: &Category) -> anyhow::Result<()>;
    async fn delete(&self, id: &CategoryId) -> anyhow::Result<()>;
    /// Les règles, de la plus ancienne à la plus récente.
    async fn read_rules(&self) -> anyhow::Result<Vec<CategoryRule>>;
    async fn read_rule(&self, id: &CategoryRuleId) -> anyhow::Result<Option<CategoryRule>>;
    async fn write_rule(&self, rule: &CategoryRule) -> anyhow::Result<()>;
    async fn delete_rule(&self, id: &CategoryRuleId) -> anyhow::Result<()>;
    async fn delete_rules_of_category(&self, category_id: &CategoryId) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, category: &str) -> CategoryRule {
        CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: category.into(),
            pattern: pattern.into(),
        }
    }

    #[test]
    fn le_motif_se_cherche_sans_casse_sans_accents_ni_espaces_multiples() {
        let rule = rule("  Électricité ", "énergie");
        assert!(rule.matches("PRLV   ELECTRICITE de France"));
        assert!(rule.matches("prlv électricité"));
        assert!(!rule.matches("PRLV GAZ"));
    }

    #[test]
    fn la_regle_la_plus_precise_l_emporte() {
        let rules = [
            rule("carrefour", "courses"),
            rule("carrefour city", "depannage"),
        ];
        assert_eq!(
            matching_category("CB CARREFOUR CITY 0412", &rules),
            Some("depannage".into())
        );
        assert_eq!(
            matching_category("CB CARREFOUR 0412", &rules),
            Some("courses".into())
        );
    }

    #[test]
    fn a_precision_egale_la_plus_ancienne_gagne() {
        let first = rule("loyer", "logement");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = rule("loyer", "autre");
        // Même résultat quel que soit l'ordre dans lequel les règles sont fournies.
        assert_eq!(
            matching_category("LOYER", &[second.clone(), first.clone()]),
            Some("logement".into())
        );
        assert_eq!(
            matching_category("LOYER", &[first, second]),
            Some("logement".into())
        );
    }

    #[test]
    fn aucune_regle_ne_convient() {
        assert_eq!(matching_category("SALAIRE", &[rule("loyer", "x")]), None);
        assert_eq!(matching_category("SALAIRE", &[]), None);
    }

    #[test]
    fn valide_le_nom_et_le_motif() {
        assert_eq!(Category::validate_name("  Courses "), Ok("Courses".into()));
        assert!(Category::validate_name("  ").is_err());
        assert!(Category::validate_name(&"x".repeat(61)).is_err());
        assert_eq!(
            CategoryRule::validate_pattern(" loyer "),
            Ok("loyer".into())
        );
        assert!(CategoryRule::validate_pattern("   ").is_err());
        assert!(CategoryRule::validate_pattern(&"x".repeat(101)).is_err());
    }

    #[test]
    fn compare_les_noms_sans_casse_ni_accents() {
        let category = Category {
            id: "c".into(),
            name: "Énergie".into(),
            kind: CategoryKind::Expense,
        };
        assert!(category.same_name("energie"));
        assert!(!category.same_name("eau"));
    }

    #[test]
    fn le_code_du_sens_fait_l_aller_retour() {
        for kind in CategoryKind::ALL {
            assert_eq!(CategoryKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(CategoryKind::from_code("inconnu"), None);
    }

    #[test]
    fn les_categories_usuelles_ont_des_noms_valides_et_uniques() {
        let mut names: Vec<_> = Category::DEFAULTS
            .iter()
            .map(|(name, _)| {
                assert!(Category::validate_name(name).is_ok());
                normalize(name)
            })
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), Category::DEFAULTS.len());
    }
}
