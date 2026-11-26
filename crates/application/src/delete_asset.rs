use std::sync::Arc;

use asset::{Asset, AssetProvider};
use bank::BankConnectionProvider;
use budget::BudgetProvider;
use recurrence::RecurrenceProvider;
use shared::AssetId;
use transaction::TransactionProvider;
use valuation::ValuationProvider;

use crate::{NOT_FOUND, UseCaseError};

pub struct DeleteAsset {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub budget_provider: Arc<dyn BudgetProvider>,
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
}

pub struct Command {
    pub asset_id: AssetId,
}

pub struct Output {
    /// L'actif tel qu'il était avant sa suppression, pour confirmer ce qui a été supprimé.
    pub asset: Asset,
}

impl DeleteAsset {
    /// Supprime l'actif, tout son historique de valorisations et ses transactions.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let asset = self
            .asset_provider
            .read(&command.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        // L'historique et les transactions d'abord : la base refuse une ligne sans actif.
        // Limite connue : les deux suppressions ne sont pas dans une même transaction. Si la seconde
        // échoue, l'actif reste sans historique. Accepté pour l'instant (SQLite local, un seul utilisateur) ;
        // à terme, une unité de travail transactionnelle dans les providers (docs/tech/architecture.md).
        // Un livret de destination supprimé emporte le réglage de l'épargne (la base refuse un réglage orphelin).
        if let Some(plan) = self.budget_provider.read_savings_plan().await?
            && plan.destination == asset.id
        {
            self.budget_provider.delete_savings_plan().await?;
        }
        self.bank_provider.unlink_asset(&asset.id).await?;
        self.recurrence_provider
            .delete_of_account(&asset.id)
            .await?;
        self.valuation_provider.delete_of_asset(&asset.id).await?;
        self.transaction_provider.delete_of_asset(&asset.id).await?;
        self.asset_provider.delete(&asset.id).await?;
        Ok(Output { asset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::{
        InMemoryAssets, InMemoryBudget, InMemoryRecurrence, InMemoryTransactions,
        InMemoryValuations,
    };
    use valuation::Valuation;

    #[tokio::test]
    async fn supprime_l_actif_et_son_historique_sans_toucher_aux_autres() {
        let asset = |id: &str| Asset {
            id: id.into(),
            name: id.into(),
            kind: AssetKind::Crypto,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        let valuation = |id: &str| Valuation {
            id: shared::ValuationId::generate(),
            asset_id: id.into(),
            value: Money::new(Decimal::ONE, Currency::Eur),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
        };
        let assets = Arc::new(InMemoryAssets::with([asset("a"), asset("b")]));
        let valuations = Arc::new(InMemoryValuations::with([valuation("a"), valuation("b")]));

        DeleteAsset {
            asset_provider: assets.clone(),
            valuation_provider: valuations.clone(),
            transaction_provider: Arc::new(InMemoryTransactions::default()),
            budget_provider: Arc::new(InMemoryBudget::default()),
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
            bank_provider: Arc::new(testing::InMemoryBank::default()),
        }
        .handle(Command {
            asset_id: "a".into(),
        })
        .await
        .unwrap();

        assert_eq!(assets.read_all().await.unwrap(), vec![asset("b")]);
        let remaining: Vec<_> = valuations
            .read_all()
            .await
            .unwrap()
            .into_iter()
            .map(|v| v.asset_id)
            .collect();
        assert_eq!(remaining, vec![AssetId::from("b")]);
    }

    #[tokio::test]
    async fn refuse_un_actif_inconnu() {
        let use_case = DeleteAsset {
            asset_provider: Arc::new(InMemoryAssets::default()),
            valuation_provider: Arc::new(InMemoryValuations::default()),
            transaction_provider: Arc::new(InMemoryTransactions::default()),
            budget_provider: Arc::new(InMemoryBudget::default()),
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
            bank_provider: Arc::new(testing::InMemoryBank::default()),
        };
        assert!(matches!(
            use_case
                .handle(Command {
                    asset_id: "x".into()
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[tokio::test]
    async fn supprimer_le_livret_de_destination_emporte_le_reglage_de_l_epargne() {
        let book = Asset {
            id: "livret".into(),
            name: "Livret A".into(),
            kind: AssetKind::SavingsBook,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        let plan = |destination: &str| budget::SavingsPlan {
            destination: destination.into(),
            ceiling: Money::new(Decimal::from(22_950), Currency::Eur),
            cushion_override: None,
        };
        let delete = |budget: Arc<InMemoryBudget>| DeleteAsset {
            asset_provider: Arc::new(InMemoryAssets::with([book.clone()])),
            valuation_provider: Arc::new(InMemoryValuations::default()),
            transaction_provider: Arc::new(InMemoryTransactions::default()),
            budget_provider: budget,
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
            bank_provider: Arc::new(testing::InMemoryBank::default()),
        };
        let command = || Command {
            asset_id: "livret".into(),
        };

        let linked = Arc::new(InMemoryBudget::with([], Some(plan("livret"))));
        delete(linked.clone()).handle(command()).await.unwrap();
        assert_eq!(linked.read_savings_plan().await.unwrap(), None);

        // Le réglage d'un autre livret n'est pas touché.
        let other = Arc::new(InMemoryBudget::with([], Some(plan("autre"))));
        delete(other.clone()).handle(command()).await.unwrap();
        assert!(other.read_savings_plan().await.unwrap().is_some());
    }
}
