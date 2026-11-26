use std::sync::Arc;

use category::{CategoryProvider, normalize};
use recurrence::{Direction, PlannedPayment, RecurrenceProvider, Window};
use shared::{AssetId, PlannedPaymentId};
use transaction::TransactionProvider;

use crate::UseCaseError;

pub struct AcceptRecurrenceSuggestion {
    pub category_provider: Arc<dyn CategoryProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Command {
    /// Ce qui désigne la suggestion : son compte, son mot du libellé et son sens.
    /// Le rythme et le montant ne viennent **pas** du formulaire : ils sont recalculés d'après l'historique.
    pub account_id: AssetId,
    pub pattern: String,
    pub direction: Direction,
}

pub struct Output {
    pub payment: PlannedPayment,
}

impl AcceptRecurrenceSuggestion {
    /// Crée le paiement que la détection propose. Rien n'est créé d'office : c'est ce cas d'usage, appelé
    /// par un clic, qui valide la suggestion.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let categories = self.category_provider.read_all().await?;
        let transactions = self.transaction_provider.read_all().await?;
        let payments = self.recurrence_provider.read_all().await?;

        let suggestion = service::recurrence::detect(&transactions, &categories, &payments)
            .into_iter()
            .find(|s| {
                s.account_id == command.account_id
                    && s.direction == command.direction
                    && normalize(&s.pattern) == normalize(&command.pattern)
            })
            .ok_or_else(|| {
                UseCaseError::refusal(
                    "Cette suggestion n'existe plus : elle a peut-être déjà été ajoutée, ou l'historique a changé.",
                )
            })?;
        let payment = PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: suggestion.label,
            pattern: suggestion.pattern,
            direction: suggestion.direction,
            account_id: suggestion.account_id,
            category_id: None,
            schedule: suggestion.schedule,
            amount: suggestion.amount,
            window: Window::default(),
        };
        self.recurrence_provider.write(&payment).await?;
        Ok(Output { payment })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money, TransactionId};
    use testing::{InMemoryCategories, InMemoryRecurrence, InMemoryTransactions};
    use transaction::{Transaction, TransactionSource};

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn setup() -> (AcceptRecurrenceSuggestion, Arc<InMemoryRecurrence>) {
        let transactions: Vec<_> = (8..=11)
            .map(|m| Transaction {
                id: TransactionId::generate(),
                asset_id: "compte".into(),
                booked_on: day(m, 9),
                amount: Money::new(Decimal::from(-12), Currency::Eur),
                label: "PRLV NETFLIX".into(),
                source: TransactionSource::Csv,
                category_id: None,
            })
            .collect();
        let provider = Arc::new(InMemoryRecurrence::default());
        (
            AcceptRecurrenceSuggestion {
                category_provider: Arc::new(InMemoryCategories::default()),
                transaction_provider: Arc::new(InMemoryTransactions::with(transactions)),
                recurrence_provider: provider.clone(),
            },
            provider,
        )
    }

    fn command(pattern: &str) -> Command {
        Command {
            account_id: "compte".into(),
            pattern: pattern.into(),
            direction: Direction::Out,
        }
    }

    #[tokio::test]
    async fn cree_le_paiement_detecte_puis_ne_le_propose_plus() {
        let (use_case, provider) = setup();
        let output = use_case.handle(command("NETFLIX")).await.unwrap();
        assert_eq!(output.payment.label, "Netflix");
        assert_eq!(provider.read_all().await.unwrap().len(), 1);

        // Déjà ajouté : la suggestion a disparu.
        assert!(matches!(
            use_case.handle(command("netflix")).await,
            Err(UseCaseError::Refusal(_))
        ));
        assert_eq!(provider.read_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuse_une_suggestion_qui_n_existe_pas() {
        let (use_case, provider) = setup();
        for refused in [
            command("spotify"),
            Command {
                direction: Direction::In,
                ..command("netflix")
            },
            Command {
                account_id: "autre".into(),
                ..command("netflix")
            },
        ] {
            assert!(matches!(
                use_case.handle(refused).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert!(provider.read_all().await.unwrap().is_empty());
    }
}
