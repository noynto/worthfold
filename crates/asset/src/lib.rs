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
    /// La banque qui tient le compte (un compte bancaire seulement).
    pub bank_name: Option<String>,
    /// Le numéro du compte, normalisé : un IBAN en majuscules sans espaces, ou un autre numéro (un compte bancaire
    /// seulement). Voir [`Asset::validate_account_number`].
    pub account_number: Option<String>,
}

impl Asset {
    pub const NAME_MAX_LENGTH: usize = 100;
    pub const BANK_NAME_MAX_LENGTH: usize = 100;
    const ACCOUNT_NUMBER_MIN_LENGTH: usize = 4;
    const ACCOUNT_NUMBER_MAX_LENGTH: usize = 34;

    /// Le nom de la banque nettoyé, ou `None` s'il est vide.
    pub fn validate_bank_name(name: &str) -> Result<Option<String>, String> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        if name.chars().count() > Self::BANK_NAME_MAX_LENGTH {
            return Err(format!(
                "Le nom de la banque ne doit pas dépasser {} caractères.",
                Self::BANK_NAME_MAX_LENGTH
            ));
        }
        Ok(Some(name.to_owned()))
    }

    /// Le numéro de compte normalisé (espaces et tirets retirés, majuscules), ou `None` s'il est vide.
    /// Un numéro qui commence par deux lettres et deux chiffres est lu comme un IBAN : sa clé de contrôle
    /// est vérifiée, ce qui attrape une faute de frappe. Tout autre numéro (RIB, compte étranger) est accepté
    /// tel quel, s'il est alphanumérique.
    pub fn validate_account_number(number: &str) -> Result<Option<String>, String> {
        let number: String = number
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if number.is_empty() {
            return Ok(None);
        }
        let length = number.chars().count();
        if !number.chars().all(|c| c.is_ascii_alphanumeric())
            || !(Self::ACCOUNT_NUMBER_MIN_LENGTH..=Self::ACCOUNT_NUMBER_MAX_LENGTH)
                .contains(&length)
        {
            return Err(format!(
                "Le numéro de compte ne contient que des lettres et des chiffres, de {} à {} caractères.",
                Self::ACCOUNT_NUMBER_MIN_LENGTH,
                Self::ACCOUNT_NUMBER_MAX_LENGTH
            ));
        }
        if looks_like_iban(&number) && !iban_checksum_is_valid(&number) {
            return Err(
                "Cet IBAN n'est pas valide : vérifie qu'il est recopié sans erreur.".into(),
            );
        }
        Ok(Some(number))
    }

    /// Le numéro lisible, par groupes de quatre : `FR76 3000 6000 …`.
    pub fn format_account_number(number: &str) -> String {
        let chars: Vec<char> = number.chars().collect();
        chars
            .chunks(4)
            .map(|group| group.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Le numéro masqué, pour les listes : seuls les quatre derniers caractères restent visibles.
    pub fn mask_account_number(number: &str) -> String {
        let chars: Vec<char> = number.chars().collect();
        let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
        format!("•••• {tail}")
    }

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

/// Deux lettres (le pays) puis deux chiffres (la clé de contrôle).
fn looks_like_iban(number: &str) -> bool {
    let mut chars = number.chars();
    let letters = chars.by_ref().take(2).all(|c| c.is_ascii_alphabetic());
    let digits = chars.take(2).filter(char::is_ascii_digit).count() == 2;
    letters && digits
}

/// Contrôle ISO 7064 (modulo 97) : les quatre premiers caractères passent à la fin, chaque lettre devient
/// un nombre (A = 10 … Z = 35), et le reste de la division par 97 doit être 1.
fn iban_checksum_is_valid(iban: &str) -> bool {
    let rotated = iban.chars().skip(4).chain(iban.chars().take(4));
    let mut remainder: u32 = 0;
    for c in rotated {
        let Some(value) = c.to_digit(36) else {
            return false;
        };
        remainder = if value >= 10 {
            (remainder * 100 + value) % 97
        } else {
            (remainder * 10 + value) % 97
        };
    }
    remainder == 1
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

#[cfg(test)]
mod bank_tests {
    use super::*;

    #[test]
    fn un_iban_valide_est_normalise() {
        assert_eq!(
            Asset::validate_account_number(" fr14 2004 1010 0505 0001 3m02 606 "),
            Ok(Some("FR1420041010050500013M02606".into()))
        );
        assert_eq!(
            Asset::validate_account_number("GB82-WEST-1234-5698-7654-32"),
            Ok(Some("GB82WEST12345698765432".into()))
        );
    }

    #[test]
    fn une_faute_de_frappe_dans_un_iban_est_refusee() {
        let error = Asset::validate_account_number("FR1420041010050500013M02607").unwrap_err();
        assert!(error.contains("IBAN"));
    }

    #[test]
    fn un_numero_qui_n_est_pas_un_iban_est_accepte_tel_quel() {
        assert_eq!(
            Asset::validate_account_number("12345 67890"),
            Ok(Some("1234567890".into()))
        );
        assert!(Asset::validate_account_number("12").is_err());
        assert!(Asset::validate_account_number("abc/123").is_err());
        assert!(Asset::validate_account_number(&"1".repeat(35)).is_err());
    }

    #[test]
    fn vide_signifie_non_renseigne() {
        assert_eq!(Asset::validate_account_number("  "), Ok(None));
        assert_eq!(Asset::validate_bank_name("  "), Ok(None));
        assert_eq!(
            Asset::validate_bank_name(" Crédit Agricole "),
            Ok(Some("Crédit Agricole".into()))
        );
        assert!(Asset::validate_bank_name(&"a".repeat(101)).is_err());
    }

    #[test]
    fn le_numero_se_groupe_et_se_masque() {
        assert_eq!(
            Asset::format_account_number("FR1420041010050500013M02606"),
            "FR14 2004 1010 0505 0001 3M02 606"
        );
        assert_eq!(
            Asset::mask_account_number("FR1420041010050500013M02606"),
            "•••• 2606"
        );
        assert_eq!(Asset::mask_account_number("12"), "•••• 12");
    }
}
