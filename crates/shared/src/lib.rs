//! Identifiants et petites valeurs partagés entre les domaines.

use std::fmt;
use std::ops::Add;
use std::str::FromStr;

use rust_decimal::Decimal;

/// Déclare un identifiant : une chaîne opaque (UUID v7, trié par date de création).
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn generate() -> Self {
                Self(uuid::Uuid::now_v7().to_string())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

/// Une valeur aléatoire imprévisible (122 bits), pour ce qui ne doit pas se deviner : l'état d'un retour de banque.
pub fn random_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

id_type!(
    /// Identifiant d'un actif (UUID v7 : trié par date de création).
    AssetId
);
id_type!(
    /// Identifiant d'une valorisation (UUID v7 : trié par date de saisie, ce qui départage deux valeurs du même jour).
    ValuationId
);
id_type!(
    /// Identifiant d'une transaction bancaire (UUID v7 : trié par date d'import).
    TransactionId
);
id_type!(
    /// Identifiant d'une catégorie de transactions.
    CategoryId
);
id_type!(
    /// Identifiant d'une règle d'affectation de catégorie.
    CategoryRuleId
);
id_type!(
    /// Identifiant d'un consentement bancaire (UUID v7 : trié par date de création).
    BankConnectionId
);
id_type!(
    /// Identifiant d'un paiement prévu (UUID v7 : trié par date de création).
    PlannedPaymentId
);
id_type!(
    /// Identifiant d'une enveloppe de budget (UUID v7 : trié par date de saisie, ce qui départage deux plafonds du même mois).
    EnvelopeId
);

/// Devise d'un montant. Seul l'euro est proposé pour l'instant (docs/specs/net-worth.md) ;
/// le reste du code gère déjà plusieurs devises : en ajouter une, c'est une variante ici,
/// son code ci-dessous et son symbole dans `server::format::symbol`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Currency {
    Eur,
}

impl Currency {
    pub const ALL: [Currency; 1] = [Currency::Eur];

    pub fn code(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
        }
    }
}

impl FromStr for Currency {
    type Err = UnknownCurrency;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Currency::ALL
            .into_iter()
            .find(|c| c.code().eq_ignore_ascii_case(s))
            .ok_or_else(|| UnknownCurrency(s.to_owned()))
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("La devise {0} n'est pas prise en charge.")]
pub struct UnknownCurrency(pub String);

/// Un montant dans une devise. Jamais de `f64` pour de l'argent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    pub fn zero(currency: Currency) -> Self {
        Self::new(Decimal::ZERO, currency)
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("Impossible d'additionner des {left} et des {right} sans conversion.")]
pub struct CurrencyMismatch {
    pub left: Currency,
    pub right: Currency,
}

/// L'addition refuse de mélanger deux devises : l'erreur est dans le type.
impl Add for Money {
    type Output = Result<Money, CurrencyMismatch>;

    fn add(self, other: Money) -> Self::Output {
        if self.currency != other.currency {
            return Err(CurrencyMismatch {
                left: self.currency,
                right: other.currency,
            });
        }
        Ok(Money::new(self.amount + other.amount, self.currency))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.amount.round_dp(2), self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additionne_deux_montants_de_meme_devise() {
        let total = Money::new(Decimal::new(1050, 2), Currency::Eur)
            + Money::new(Decimal::new(250, 2), Currency::Eur);
        assert_eq!(total, Ok(Money::new(Decimal::new(1300, 2), Currency::Eur)));
    }

    // Avec une deuxième devise, rétablir les tests du refus de mélanger les devises : ici
    // (addition), dans read_net_worth et read_net_worth_history (total incalculable) et dans
    // update_asset (devise figée d'un actif valorisé).
}
