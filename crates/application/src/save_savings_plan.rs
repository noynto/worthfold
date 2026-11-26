use std::sync::Arc;

use asset::{AssetKind, AssetProvider};
use budget::{BudgetProvider, SavingsPlan};
use rust_decimal::Decimal;
use shared::{AssetId, Currency, Money};

use crate::{NOT_FOUND, UseCaseError};

pub struct SaveSavingsPlan {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub budget_provider: Arc<dyn BudgetProvider>,
}

pub struct Command {
    /// Le livret où verser ce qui reste en fin de mois.
    pub destination: AssetId,
    /// Le plafond de dépôt du livret.
    pub ceiling: Decimal,
    /// Un matelas fixé à la main ; `None` pour qu'il soit évalué d'après les dépenses.
    pub cushion: Option<Decimal>,
    pub currency: Currency,
}

pub struct Output {
    pub plan: SavingsPlan,
}

impl SaveSavingsPlan {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let destination = self
            .asset_provider
            .read(&command.destination)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        if destination.kind != AssetKind::SavingsBook {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'est pas un livret d'épargne : choisis un livret.",
                destination.name
            )));
        }
        if destination.currency != command.currency {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'est pas en {} : le budget et le livret doivent avoir la même devise.",
                destination.name, command.currency
            )));
        }
        if command.ceiling <= Decimal::ZERO {
            return Err(UseCaseError::refusal(
                "Le plafond du livret doit être supérieur à zéro.",
            ));
        }
        if command
            .cushion
            .is_some_and(|c| c.is_sign_negative() && !c.is_zero())
        {
            return Err(UseCaseError::refusal(
                "Le matelas ne peut pas être négatif.",
            ));
        }
        let plan = SavingsPlan {
            destination: destination.id,
            ceiling: Money::new(command.ceiling, command.currency),
            cushion_override: command.cushion.map(|c| Money::new(c, command.currency)),
        };
        self.budget_provider.write_savings_plan(&plan).await?;
        Ok(Output { plan })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::Asset;
    use testing::{InMemoryAssets, InMemoryBudget};

    fn setup() -> (SaveSavingsPlan, Arc<InMemoryBudget>) {
        let asset = |id: &str, kind| Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
        };
        let budget = Arc::new(InMemoryBudget::default());
        (
            SaveSavingsPlan {
                asset_provider: Arc::new(InMemoryAssets::with([
                    asset("livret", AssetKind::SavingsBook),
                    asset("compte", AssetKind::BankAccount),
                ])),
                budget_provider: budget.clone(),
            },
            budget,
        )
    }

    fn command(destination: &str, ceiling: i64, cushion: Option<i64>) -> Command {
        Command {
            destination: destination.into(),
            ceiling: Decimal::from(ceiling),
            cushion: cushion.map(Decimal::from),
            currency: Currency::Eur,
        }
    }

    #[tokio::test]
    async fn enregistre_le_reglage_avec_ou_sans_matelas_fixe() {
        let (use_case, budget) = setup();
        let output = use_case
            .handle(command("livret", 22_950, None))
            .await
            .unwrap();
        assert_eq!(output.plan.cushion_override, None);

        use_case
            .handle(command("livret", 22_950, Some(1_500)))
            .await
            .unwrap();
        let stored = budget.read_savings_plan().await.unwrap().unwrap();
        assert_eq!(
            stored.cushion_override,
            Some(Money::new(Decimal::from(1_500), Currency::Eur))
        );
    }

    #[tokio::test]
    async fn refuse_un_actif_qui_n_est_pas_un_livret_ou_inconnu() {
        let (use_case, budget) = setup();
        for destination in ["compte", "inconnu"] {
            assert!(matches!(
                use_case.handle(command(destination, 22_950, None)).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert_eq!(budget.read_savings_plan().await.unwrap(), None);
    }

    #[tokio::test]
    async fn refuse_un_plafond_nul_ou_un_matelas_negatif() {
        let (use_case, _) = setup();
        assert!(use_case.handle(command("livret", 0, None)).await.is_err());
        assert!(use_case.handle(command("livret", -5, None)).await.is_err());
        assert!(
            use_case
                .handle(command("livret", 22_950, Some(-1)))
                .await
                .is_err()
        );
        assert!(
            use_case
                .handle(command("livret", 22_950, Some(0)))
                .await
                .is_ok()
        );
    }
}
