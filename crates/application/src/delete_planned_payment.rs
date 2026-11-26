use std::sync::Arc;

use recurrence::{PlannedPayment, RecurrenceProvider};
use shared::PlannedPaymentId;

use crate::UseCaseError;

pub(crate) const PAYMENT_NOT_FOUND: &str =
    "Ce paiement n'existe plus : il a peut-être été supprimé.";

pub struct DeletePlannedPayment {
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Command {
    pub payment_id: PlannedPaymentId,
}

pub struct Output {
    /// Le paiement tel qu'il était, pour confirmer ce qui a été supprimé.
    pub payment: PlannedPayment,
}

impl DeletePlannedPayment {
    /// Supprime le paiement prévu, ses paliers de montant et ses exceptions. Les opérations réelles
    /// qu'il reconnaissait restent dans le compte : elles ne sont qu'une trace de ce qui s'est passé.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let payment = self
            .recurrence_provider
            .read(&command.payment_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(PAYMENT_NOT_FOUND))?;
        self.recurrence_provider.delete(&payment.id).await?;
        Ok(Output { payment })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use recurrence::{
        Amount, AmountStep, DayOfMonth, Direction, ExceptionAction, Schedule, Window,
    };
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::InMemoryRecurrence;

    fn payment() -> PlannedPayment {
        let eur = Money::new(Decimal::ONE, Currency::Eur);
        PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: "Loyer".into(),
            pattern: "loyer".into(),
            direction: Direction::Out,
            account_id: "compte".into(),
            category_id: None,
            schedule: Schedule::Monthly {
                day: DayOfMonth::Day(5),
            },
            amount: Amount {
                steps: vec![AmountStep {
                    from: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    value: eur,
                    margin: Money::zero(Currency::Eur),
                }],
                revision: None,
                observed: false,
            },
            window: Window::default(),
        }
    }

    #[tokio::test]
    async fn supprime_le_paiement_et_ses_exceptions_sans_toucher_aux_autres() {
        let (a, b) = (payment(), payment());
        let provider = Arc::new(InMemoryRecurrence::with([a.clone(), b.clone()]));
        let day = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        provider
            .write_exception(&a.id, day, ExceptionAction::Skip)
            .await
            .unwrap();
        provider
            .write_exception(&b.id, day, ExceptionAction::Skip)
            .await
            .unwrap();

        DeletePlannedPayment {
            recurrence_provider: provider.clone(),
        }
        .handle(Command { payment_id: a.id })
        .await
        .unwrap();

        assert_eq!(provider.read_all().await.unwrap(), vec![b.clone()]);
        let exceptions = provider.read_exceptions().await.unwrap();
        assert_eq!(exceptions.len(), 1);
        assert_eq!(exceptions[0].0, b.id);
    }

    #[tokio::test]
    async fn refuse_un_paiement_inconnu() {
        let use_case = DeletePlannedPayment {
            recurrence_provider: Arc::new(InMemoryRecurrence::default()),
        };
        assert!(matches!(
            use_case
                .handle(Command {
                    payment_id: PlannedPaymentId::generate()
                })
                .await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
