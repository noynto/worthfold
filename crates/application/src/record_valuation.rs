use std::sync::Arc;

use asset::AssetProvider;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{AssetId, Money};
use valuation::{Valuation, ValuationProvider};

use crate::{NOT_FOUND, UseCaseError};

pub struct RecordValuation {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
}

pub struct Command {
    pub asset_id: AssetId,
    /// Le montant tel que saisi. Pour un prêt : le capital restant dû, en positif.
    /// Pour un avoir, il peut être négatif (un compte à découvert).
    pub amount: Decimal,
    pub valued_on: NaiveDate,
    /// La date du jour, fournie par l'appelant (une date future est refusée ; testable sans horloge).
    pub today: NaiveDate,
}

pub struct Output {
    pub valuation: Valuation,
}

impl RecordValuation {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        // 0. Vérification de la commande : une valeur future serait ignorée par le total, sans explication.
        if command.valued_on > command.today {
            return Err(UseCaseError::refusal(
                "La date de valorisation ne peut pas être dans le futur.",
            ));
        }
        // 1. Résolution de l'actif : la valorisation prend sa devise.
        let asset = self
            .asset_provider
            .read(&command.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        // 2. Règle : une dette se saisit en positif et s'enregistre en négatif.
        let amount = if asset.kind.is_liability() {
            if command.amount.is_sign_negative() && !command.amount.is_zero() {
                return Err(UseCaseError::refusal(format!(
                    "« {} » est un prêt : saisis le capital restant dû en positif, il sera déduit du patrimoine.",
                    asset.name
                )));
            }
            -command.amount
        } else {
            command.amount
        };
        // 3. Écriture : une nouvelle ligne d'historique, jamais une mise à jour.
        let valuation = Valuation {
            id: shared::ValuationId::generate(),
            asset_id: asset.id,
            value: Money::new(amount, asset.currency),
            valued_on: command.valued_on,
        };
        self.valuation_provider.write(&valuation).await?;
        Ok(Output { valuation })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use shared::Currency;
    use testing::{InMemoryAssets, InMemoryValuations};

    #[tokio::test]
    async fn enregistre_une_valorisation_dans_la_devise_de_l_actif() {
        let asset = Asset {
            id: "a".into(),
            name: "Compte courant".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        let valuations = Arc::new(InMemoryValuations::default());
        let use_case = RecordValuation {
            asset_provider: Arc::new(InMemoryAssets::with([asset])),
            valuation_provider: valuations.clone(),
        };

        let day = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let output = use_case
            .handle(Command {
                asset_id: "a".into(),
                amount: Decimal::from(42),
                valued_on: day,
                today: day,
            })
            .await
            .unwrap();

        assert_eq!(
            output.valuation.value,
            Money::new(Decimal::from(42), Currency::Eur)
        );
        assert_eq!(valuations.read_all().await.unwrap().len(), 1);
    }

    fn use_case_with(kind: AssetKind) -> RecordValuation {
        let asset = Asset {
            id: "a".into(),
            name: "Actif".into(),
            kind,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        RecordValuation {
            asset_provider: Arc::new(InMemoryAssets::with([asset])),
            valuation_provider: Arc::new(InMemoryValuations::default()),
        }
    }

    fn command(amount: i64) -> Command {
        Command {
            asset_id: "a".into(),
            amount: Decimal::from(amount),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            today: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
        }
    }

    #[tokio::test]
    async fn refuse_une_date_future() {
        let mut future = command(100);
        future.valued_on = future.today.succ_opt().unwrap();
        let result = use_case_with(AssetKind::BankAccount).handle(future).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }

    #[tokio::test]
    async fn enregistre_un_pret_en_negatif() {
        let output = use_case_with(AssetKind::Loan)
            .handle(command(182_340))
            .await
            .unwrap();
        assert_eq!(
            output.valuation.value,
            Money::new(Decimal::from(-182_340), Currency::Eur)
        );
    }

    #[tokio::test]
    async fn refuse_un_pret_saisi_en_negatif() {
        let result = use_case_with(AssetKind::Loan)
            .handle(command(-182_340))
            .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }

    #[tokio::test]
    async fn accepte_un_avoir_negatif() {
        let output = use_case_with(AssetKind::BankAccount)
            .handle(command(-250))
            .await
            .unwrap();
        assert_eq!(
            output.valuation.value,
            Money::new(Decimal::from(-250), Currency::Eur)
        );
    }

    #[tokio::test]
    async fn refuse_un_actif_inconnu() {
        let use_case = RecordValuation {
            asset_provider: Arc::new(InMemoryAssets::default()),
            valuation_provider: Arc::new(InMemoryValuations::default()),
        };
        let day = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let result = use_case
            .handle(Command {
                asset_id: "inconnu".into(),
                amount: Decimal::ONE,
                valued_on: day,
                today: day,
            })
            .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
    }
}
