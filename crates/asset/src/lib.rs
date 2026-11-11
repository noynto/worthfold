//! Domaine des actifs : ce que l'on possède.

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use shared::{AssetId, Currency};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub id: AssetId,
    pub name: String,
    pub kind: AssetKind,
    pub currency: Currency,
}

impl Asset {
    pub const NAME_MAX_LENGTH: usize = 100;

    /// Le nom nettoyé, ou le refus en français. Règle propre aux actifs : elle vit ici,
    /// et sert à la création comme à la modification.
    pub fn validate_name(name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Le nom de l'actif est nécessaire.".into());
        }
        if name.chars().count() > Self::NAME_MAX_LENGTH {
            return Err(format!(
                "Le nom de l'actif ne doit pas dépasser {} caractères.",
                Self::NAME_MAX_LENGTH
            ));
        }
        Ok(name.to_owned())
    }
}

/// Les grandes familles d'actifs. Ajouter une variante fait échouer la compilation
/// de chaque `match` qui ne la traite pas encore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    BankAccount,
    SavingsBook,
    LifeInsurance,
    Securities,
    RealEstate,
    Crypto,
    /// Une dette : sa valeur diminue le patrimoine.
    Loan,
}

impl AssetKind {
    pub const ALL: [AssetKind; 7] = [
        AssetKind::BankAccount,
        AssetKind::SavingsBook,
        AssetKind::LifeInsurance,
        AssetKind::Securities,
        AssetKind::RealEstate,
        AssetKind::Crypto,
        AssetKind::Loan,
    ];

    /// Vrai pour une dette. Sa valeur se saisit en positif (le capital restant dû)
    /// et s'enregistre en négatif.
    pub fn is_liability(self) -> bool {
        match self {
            AssetKind::Loan => true,
            AssetKind::BankAccount
            | AssetKind::SavingsBook
            | AssetKind::LifeInsurance
            | AssetKind::Securities
            | AssetKind::RealEstate
            | AssetKind::Crypto => false,
        }
    }

    /// Code stable, utilisé en base et dans les formulaires.
    pub fn code(self) -> &'static str {
        match self {
            AssetKind::BankAccount => "bank_account",
            AssetKind::SavingsBook => "savings_book",
            AssetKind::LifeInsurance => "life_insurance",
            AssetKind::Securities => "securities",
            AssetKind::RealEstate => "real_estate",
            AssetKind::Crypto => "crypto",
            AssetKind::Loan => "loan",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AssetKind::BankAccount => "Compte bancaire",
            AssetKind::SavingsBook => "Livret d'épargne",
            AssetKind::LifeInsurance => "Assurance vie",
            AssetKind::Securities => "Titres (PEA, compte-titres)",
            AssetKind::RealEstate => "Immobilier",
            AssetKind::Crypto => "Crypto-actifs",
            AssetKind::Loan => "Prêt",
        }
    }
}

impl FromStr for AssetKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AssetKind::ALL
            .into_iter()
            .find(|k| k.code() == s)
            .ok_or_else(|| format!("Le type d'actif {s} n'existe pas."))
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Port de lecture et d'écriture des actifs. Aucune règle métier ici.
#[async_trait]
pub trait AssetProvider: Send + Sync {
    async fn read_all(&self) -> anyhow::Result<Vec<Asset>>;
    async fn read(&self, id: &AssetId) -> anyhow::Result<Option<Asset>>;
    async fn write(&self, asset: &Asset) -> anyhow::Result<()>;
    async fn delete(&self, id: &AssetId) -> anyhow::Result<()>;
}
