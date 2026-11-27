//! Domaine des banques : chaque banque dit comment lire ses exports CSV.
//!
//! Les banques n'ont pas de format commun. Plutôt que de le deviner, on le décrit une fois par banque
//! ([`CsvFormat`]) ; un compte bancaire choisit sa banque, et l'import de ses relevés s'en sert.

use async_trait::async_trait;

/// Déclare une énumération à code stable (enregistré en base, envoyé par les formulaires) et libellé français.
macro_rules! coded_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => ($code:literal, $label:literal)),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn code(self) -> &'static str {
                match self { $($name::$variant => $code),+ }
            }

            pub fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            pub fn from_code(code: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|v| v.code() == code)
            }
        }
    };
}

coded_enum!(
    /// L'encodage du fichier. Les banques françaises exportent souvent en Windows-1252.
    Encoding {
        Utf8 => ("utf8", "UTF-8"),
        Windows1252 => ("windows1252", "Windows-1252 (Latin-1)"),
    }
);

coded_enum!(
    /// Le caractère qui sépare les colonnes.
    Delimiter {
        Semicolon => ("semicolon", "Point-virgule ( ; )"),
        Comma => ("comma", "Virgule ( , )"),
        Tab => ("tab", "Tabulation"),
    }
);

coded_enum!(
    /// Le caractère qui sépare les décimales dans les montants.
    DecimalSeparator {
        Comma => ("comma", "Virgule (1 234,56)"),
        Dot => ("dot", "Point (1,234.56)"),
    }
);

coded_enum!(
    /// L'écriture des dates.
    DateFormat {
        DayMonthYear => ("dmy_slash", "jj/mm/aaaa"),
        DayMonthShortYear => ("dmy2_slash", "jj/mm/aa"),
        DayMonthYearDash => ("dmy_dash", "jj-mm-aaaa"),
        DayMonthYearDot => ("dmy_dot", "jj.mm.aaaa"),
        YearMonthDay => ("ymd_dash", "aaaa-mm-jj"),
    }
);

coded_enum!(
    /// Ce qui sépare le jour et le mois d'une date sans année (`15.10` ou `15/10`) dans un relevé PDF.
    DaySeparator {
        Dot => ("dot", "Point (15.10)"),
        Slash => ("slash", "Barre oblique (15/10)"),
    }
);

impl DaySeparator {
    pub fn character(self) -> char {
        match self {
            DaySeparator::Dot => '.',
            DaySeparator::Slash => '/',
        }
    }
}

impl Delimiter {
    pub fn byte(self) -> u8 {
        match self {
            Delimiter::Semicolon => b';',
            Delimiter::Comma => b',',
            Delimiter::Tab => b'\t',
        }
    }
}

impl DateFormat {
    /// Le format `chrono` correspondant.
    pub fn pattern(self) -> &'static str {
        match self {
            DateFormat::DayMonthYear => "%d/%m/%Y",
            DateFormat::DayMonthShortYear => "%d/%m/%y",
            DateFormat::DayMonthYearDash => "%d-%m-%Y",
            DateFormat::DayMonthYearDot => "%d.%m.%Y",
            DateFormat::YearMonthDay => "%Y-%m-%d",
        }
    }
}

/// Où se lit le montant : une colonne signée, ou deux colonnes (débit et crédit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmountColumns {
    Signed(String),
    DebitCredit { debit: String, credit: String },
}

/// Comment décoder l'export CSV d'une banque. Les colonnes sont désignées par le nom de leur en-tête
/// (sans tenir compte de la casse, des accents ni des espaces en trop), pas par leur position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvFormat {
    pub encoding: Encoding,
    pub delimiter: Delimiter,
    /// Le numéro (à partir de 1) de la ligne d'en-tête : les lignes avant (nom du compte, période…) sont ignorées.
    pub header_line: u32,
    pub date_column: String,
    pub date_format: DateFormat,
    pub label_column: String,
    pub amount: AmountColumns,
    pub decimal_separator: DecimalSeparator,
    /// Pour une banque qui note les dépenses en positif : le signe du montant est inversé.
    /// Sans effet avec deux colonnes débit et crédit, où le sens est donné par la colonne.
    pub invert_sign: bool,
}

impl CsvFormat {
    pub const COLUMN_MAX_LENGTH: usize = 100;
    pub const MAX_HEADER_LINE: u32 = 100;

    /// Le format que l'on propose pour une nouvelle banque : le plus courant en France.
    pub fn suggested() -> Self {
        Self {
            encoding: Encoding::Utf8,
            delimiter: Delimiter::Semicolon,
            header_line: 1,
            date_column: "Date".into(),
            date_format: DateFormat::DayMonthYear,
            label_column: "Libellé".into(),
            amount: AmountColumns::Signed("Montant".into()),
            decimal_separator: DecimalSeparator::Comma,
            invert_sign: false,
        }
    }
}

/// Comment lire le relevé PDF d'une banque. Un PDF n'a pas de colonnes : on s'appuie sur la **position** du texte.
/// Le tableau commence sous la ligne qui porte les en-têtes « Débit » et « Crédit » ; un montant est un débit
/// ou un crédit selon la colonne où il se trouve. La période du relevé (« du 15 octobre 2026 au 15 novembre 2026 »)
/// donne l'année des dates, qui n'ont que le jour et le mois.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfFormat {
    pub date_separator: DaySeparator,
    /// Le texte de l'en-tête de la colonne des débits, tel qu'imprimé (la casse et les accents ne comptent pas).
    pub debit_header: String,
    pub credit_header: String,
    /// Les lignes de solde commencent par ce texte (« SOLDE » : « SOLDE CREDITEUR AU … »). La première est le
    /// solde de départ, la dernière le solde final : ils servent à vérifier que toutes les opérations sont lues.
    pub balance_marker: String,
    /// La ligne qui termine le tableau des opérations (« TOTAL DES OPERATIONS »).
    pub end_marker: String,
}

impl PdfFormat {
    /// Ce que l'on propose pour une nouvelle banque.
    pub fn suggested() -> Self {
        Self {
            date_separator: DaySeparator::Dot,
            debit_header: "Débit".into(),
            credit_header: "Crédit".into(),
            balance_marker: "SOLDE".into(),
            end_marker: "TOTAL DES OPERATIONS".into(),
        }
    }
}

/// Un mot d'une page PDF et sa place : `x0` et `x1` (de gauche à droite) et `y` (de bas en haut), en points.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionedWord {
    pub text: String,
    pub x0: f64,
    pub x1: f64,
    pub y: f64,
}

/// Lit un fichier PDF : les mots de chaque page, avec leur position. Un adaptateur de l'infrastructure.
pub trait PdfReader: Send + Sync {
    /// Le refus est un message en français : fichier illisible, protégé, ou sans texte (une image scannée).
    fn read(&self, bytes: &[u8]) -> Result<Vec<Vec<PositionedWord>>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Institution {
    /// Le nom, unique : il identifie la banque (un compte bancaire s'y rattache par ce nom).
    pub name: String,
    pub csv: CsvFormat,
    /// Le format du relevé PDF, pour une banque qui en fournit.
    pub pdf: Option<PdfFormat>,
}

impl Institution {
    pub const NAME_MAX_LENGTH: usize = 100;

    /// Le nom nettoyé, ou le refus en français.
    pub fn validate_name(name: &str) -> Result<String, String> {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            return Err("Le nom de la banque est nécessaire.".into());
        }
        if name.chars().count() > Self::NAME_MAX_LENGTH {
            return Err(format!(
                "Le nom de la banque ne doit pas dépasser {} caractères.",
                Self::NAME_MAX_LENGTH
            ));
        }
        if name.contains(['/', '\\']) || name.chars().any(char::is_control) {
            return Err("Le nom de la banque ne peut pas contenir « / » ni « \\ ».".into());
        }
        Ok(name)
    }

    /// Deux noms qui ne diffèrent que par la casse désignent la même banque.
    pub fn same_name(&self, other: &str) -> bool {
        self.name.to_lowercase() == other.to_lowercase()
    }
}

#[async_trait]
pub trait InstitutionProvider: Send + Sync {
    /// Par ordre alphabétique.
    async fn read_all(&self) -> anyhow::Result<Vec<Institution>>;
    async fn read(&self, name: &str) -> anyhow::Result<Option<Institution>>;
    /// Crée la banque, ou remplace son format si elle existe (le nom ne change pas).
    async fn write(&self, institution: &Institution) -> anyhow::Result<()>;
    async fn delete(&self, name: &str) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_codes_se_relisent() {
        for format in DateFormat::ALL {
            assert_eq!(DateFormat::from_code(format.code()), Some(*format));
        }
        assert_eq!(Encoding::from_code("ebcdic"), None);
    }

    #[test]
    fn nettoie_le_nom_et_refuse_ce_qui_casse_une_adresse() {
        assert_eq!(
            Institution::validate_name("  Crédit   Agricole "),
            Ok("Crédit Agricole".into())
        );
        assert!(Institution::validate_name("   ").is_err());
        assert!(Institution::validate_name("A/B").is_err());
        assert!(Institution::validate_name(&"a".repeat(101)).is_err());
    }
}
