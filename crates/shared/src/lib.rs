//! Identifiants et petites valeurs partagés entre les domaines.

use std::fmt;
use std::ops::Add;
use std::str::FromStr;

use rust_decimal::Decimal;

/// Identifiant d'un actif (UUID v7 : trié par date de création).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetId(String);

impl AssetId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for AssetId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for AssetId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifiant d'une valorisation (UUID v7 : trié par date de saisie, ce qui départage deux valeurs du même jour).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValuationId(String);

impl ValuationId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ValuationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ValuationId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Display for ValuationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Devise d'un montant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Currency {
    Eur,
    Usd,
    Chf,
}

impl Currency {
    pub const ALL: [Currency; 3] = [Currency::Eur, Currency::Usd, Currency::Chf];

    pub fn code(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
            Currency::Usd => "USD",
            Currency::Chf => "CHF",
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

    #[test]
    fn refuse_de_melanger_deux_devises() {
        let total = Money::zero(Currency::Eur) + Money::zero(Currency::Usd);
        assert!(total.is_err());
    }
}
