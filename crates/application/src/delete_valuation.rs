use std::sync::Arc;

use shared::{AssetId, ValuationId};
use valuation::{Valuation, ValuationProvider};

use crate::UseCaseError;

pub struct DeleteValuation {
    pub valuation_provider: Arc<dyn ValuationProvider>,
}

pub struct Command {
    /// L'actif de la page d'où vient la demande : la valorisation doit lui appartenir.
    pub asset_id: AssetId,
    pub valuation_id: ValuationId,
}

pub struct Output {
    /// La valorisation telle qu'elle était, pour confirmer ce qui a été supprimé.
    pub valuation: Valuation,
}

impl DeleteValuation {
    /// Supprime une valorisation saisie par erreur. Le total et l'historique se recalculent d'eux-mêmes ;
    /// la dernière supprimée, l'actif redevient « jamais valorisé » et sa devise redevient modifiable.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let valuation = self
            .valuation_provider
            .read(&command.valuation_id)
            .await?
            // Une valorisation d'un autre actif est traitée comme absente : un lien périmé ou forgé
            // ne doit pas supprimer ailleurs que sur la page affichée.
            .filter(|v| v.asset_id == command.asset_id)
            .ok_or_else(|| {
                UseCaseError::refusal(
                    "Cette valorisation n'existe plus : elle a peut-être déjà été supprimée.",
                )
            })?;
        self.valuation_provider.delete(&valuation.id).await?;
        Ok(Output { valuation })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::InMemoryValuations;

    fn valuation(asset: &str, amount: i64) -> Valuation {
        Valuation {
            id: ValuationId::generate(),
            asset_id: asset.into(),
            value: Money::new(Decimal::from(amount), Currency::Eur),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
        }
    }

    #[tokio::test]
    async fn supprime_la_valorisation_visee_et_elle_seule() {
        let (wrong, right) = (valuation("a", 999), valuation("a", 100));
        let valuations = Arc::new(InMemoryValuations::with([wrong.clone(), right.clone()]));
        let use_case = DeleteValuation {
            valuation_provider: valuations.clone(),
        };

        let output = use_case
            .handle(Command {
                asset_id: "a".into(),
                valuation_id: wrong.id.clone(),
            })
            .await
            .unwrap();

        assert_eq!(output.valuation, wrong);
        assert_eq!(valuations.read_all().await.unwrap(), vec![right]);
    }

    #[tokio::test]
    async fn refuse_une_valorisation_d_un_autre_actif() {
        let other = valuation("autre", 100);
        let valuations = Arc::new(InMemoryValuations::with([other.clone()]));
        let use_case = DeleteValuation {
            valuation_provider: valuations.clone(),
        };

        let result = use_case
            .handle(Command {
                asset_id: "a".into(),
                valuation_id: other.id.clone(),
            })
            .await;

        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert_eq!(valuations.read_all().await.unwrap(), vec![other]);
    }

    #[tokio::test]
    async fn refuse_une_valorisation_deja_supprimee() {
        let use_case = DeleteValuation {
            valuation_provider: Arc::new(InMemoryValuations::default()),
        };
        let result = use_case
            .handle(Command {
                asset_id: "a".into(),
                valuation_id: ValuationId::generate(),
            })
            .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }
}
