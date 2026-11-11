//! Services : règles qui croisent plusieurs domaines. Des fonctions pures,
//! sans provider ni entrée-sortie : des objets déjà lus en entrée, une valeur en sortie.

use asset::Asset;
use chrono::NaiveDate;
use shared::{CurrencyMismatch, Money};
use valuation::Valuation;

/// La dernière valorisation connue d'un actif à la date donnée.
pub fn latest_valuation<'a>(
    asset: &Asset,
    valuations: &'a [Valuation],
    at: NaiveDate,
) -> Option<&'a Valuation> {
    valuations
        .iter()
        .filter(|v| v.asset_id == asset.id && v.valued_on <= at)
        .max_by_key(|v| v.valued_on)
}

/// Le patrimoine net : la somme des dernières valorisations de chaque actif.
pub fn net_worth(
    assets: &[Asset],
    valuations: &[Valuation],
    at: NaiveDate,
    currency: shared::Currency,
) -> Result<Money, CurrencyMismatch> {
    assets
        .iter()
        .filter_map(|asset| latest_valuation(asset, valuations, at))
        .try_fold(Money::zero(currency), |total, v| total + v.value)
}

/// Le patrimoine net décomposé : ce que l'on possède et ce que l'on doit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Breakdown {
    /// Somme des dernières valeurs des avoirs (un compte à découvert y compte en négatif).
    pub assets: Money,
    /// Somme de ce qui reste dû sur les dettes, en positif.
    pub liabilities: Money,
}

/// Avoirs et dettes à une date, d'après la dernière valorisation de chaque actif.
pub fn breakdown(
    assets: &[Asset],
    valuations: &[Valuation],
    at: NaiveDate,
    currency: shared::Currency,
) -> Result<Breakdown, CurrencyMismatch> {
    let mut result = Breakdown {
        assets: Money::zero(currency),
        liabilities: Money::zero(currency),
    };
    for asset in assets {
        let Some(latest) = latest_valuation(asset, valuations, at) else {
            continue;
        };
        if asset.kind.is_liability() {
            // Une dette est enregistrée en négatif : on la présente comme un montant dû, positif.
            result.liabilities =
                (result.liabilities + Money::new(-latest.value.amount, latest.value.currency))?;
        } else {
            result.assets = (result.assets + latest.value)?;
        }
    }
    Ok(result)
}

/// L'évolution du patrimoine net : un point à chaque date où une valorisation a été saisie,
/// plus un dernier point à `until` pour prolonger la courbe jusqu'à aujourd'hui.
pub fn net_worth_history(
    assets: &[Asset],
    valuations: &[Valuation],
    until: NaiveDate,
    currency: shared::Currency,
) -> Result<Vec<(NaiveDate, Money)>, CurrencyMismatch> {
    let mut dates: Vec<NaiveDate> = valuations
        .iter()
        .filter(|v| v.valued_on <= until && assets.iter().any(|a| a.id == v.asset_id))
        .map(|v| v.valued_on)
        .collect();
    dates.sort();
    dates.dedup();
    if dates.last().is_some_and(|last| *last < until) {
        dates.push(until);
    }
    dates
        .into_iter()
        .map(|date| net_worth(assets, valuations, date, currency).map(|total| (date, total)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::AssetKind;
    use rust_decimal::Decimal;
    use shared::{AssetId, Currency};

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn asset(id: &str) -> Asset {
        Asset {
            id: AssetId::from(id),
            name: id.into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
        }
    }

    fn valuation(id: &str, amount: i64, d: u32) -> Valuation {
        Valuation {
            id: shared::ValuationId::generate(),
            asset_id: AssetId::from(id),
            value: eur(amount),
            valued_on: day(d),
        }
    }

    #[test]
    fn retient_la_derniere_valorisation_avant_la_date() {
        let valuations = [
            valuation("a", 100, 1),
            valuation("a", 150, 5),
            valuation("a", 999, 20),
        ];
        let latest = latest_valuation(&asset("a"), &valuations, day(10)).unwrap();
        assert_eq!(latest.value, eur(150));
    }

    #[test]
    fn retrace_l_evolution_jusqu_a_la_date_demandee() {
        let assets = [asset("a"), asset("b")];
        // Le 1er : a = 100 ; le 5 : b = 50 ; le 8 : a passe à 120. Une saisie future est ignorée.
        let valuations = [
            valuation("a", 100, 1),
            valuation("b", 50, 5),
            valuation("a", 120, 8),
            valuation("a", 999, 25),
        ];

        let history = net_worth_history(&assets, &valuations, day(10), Currency::Eur).unwrap();

        assert_eq!(
            history,
            vec![
                (day(1), eur(100)),
                (day(5), eur(150)),
                (day(8), eur(170)),
                (day(10), eur(170))
            ]
        );
    }

    #[test]
    fn separe_les_avoirs_des_dettes() {
        let loan = Asset {
            id: AssetId::from("pret"),
            name: "Prêt".into(),
            kind: AssetKind::Loan,
            currency: Currency::Eur,
        };
        let assets = [asset("a"), asset("b"), loan];
        let valuations = [
            valuation("a", 100, 1),
            valuation("b", -20, 1),
            valuation("pret", -40, 1),
        ];

        let result = breakdown(&assets, &valuations, day(10), Currency::Eur).unwrap();

        // Le découvert (-20) réduit les avoirs ; la dette est présentée en positif.
        assert_eq!(
            result,
            Breakdown {
                assets: eur(80),
                liabilities: eur(40)
            }
        );
    }

    #[test]
    fn un_historique_vide_reste_vide() {
        assert_eq!(
            net_worth_history(&[asset("a")], &[], day(10), Currency::Eur),
            Ok(vec![])
        );
    }

    #[test]
    fn additionne_les_actifs() {
        let assets = [asset("a"), asset("b"), asset("sans-valeur")];
        let valuations = [valuation("a", 100, 1), valuation("b", 50, 2)];
        assert_eq!(
            net_worth(&assets, &valuations, day(10), Currency::Eur),
            Ok(eur(150))
        );
    }
}
