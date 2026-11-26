use std::sync::Arc;

use chrono::NaiveDate;
use recurrence::{ExceptionAction, RecurrenceProvider, occurrences};
use rust_decimal::Decimal;
use shared::{Money, PlannedPaymentId};

use crate::UseCaseError;
use crate::delete_planned_payment::PAYMENT_NOT_FOUND;

pub struct SetOccurrenceException {
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub enum Request {
    /// Pas d'échéance ce jour-là.
    Skip,
    /// Un autre montant ce jour-là, exact, en positif.
    Amount(Decimal),
    /// Retirer l'exception : l'échéance redevient celle du paiement.
    Clear,
}

pub struct Command {
    pub payment_id: PlannedPaymentId,
    /// La date de l'échéance, telle que le rythme la donne.
    pub date: NaiveDate,
    pub request: Request,
}

pub struct Output {
    pub payment_label: String,
}

impl SetOccurrenceException {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let payment = self
            .recurrence_provider
            .read(&command.payment_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(PAYMENT_NOT_FOUND))?;
        // L'échéance doit exister : une date forgée ou périmée ne crée pas d'exception orpheline.
        if occurrences(&payment, &[], None, command.date, command.date).is_empty() {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'a pas d'échéance à cette date.",
                payment.label
            )));
        }
        match command.request {
            Request::Skip => {
                self.recurrence_provider
                    .write_exception(&payment.id, command.date, ExceptionAction::Skip)
                    .await?;
            }
            Request::Amount(amount) => {
                if amount.is_sign_negative() && !amount.is_zero() {
                    return Err(UseCaseError::refusal(
                        "Saisis le montant en positif : le sens du paiement donne son signe.",
                    ));
                }
                let currency = payment.amount.steps[0].value.currency;
                self.recurrence_provider
                    .write_exception(
                        &payment.id,
                        command.date,
                        ExceptionAction::Amount(Money::new(amount, currency)),
                    )
                    .await?;
            }
            Request::Clear => {
                self.recurrence_provider
                    .delete_exception(&payment.id, command.date)
                    .await?;
            }
        }
        Ok(Output {
            payment_label: payment.label,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recurrence::{Amount, AmountStep, DayOfMonth, Direction, PlannedPayment, Schedule, Window};
    use shared::Currency;
    use testing::InMemoryRecurrence;

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn setup() -> (
        SetOccurrenceException,
        Arc<InMemoryRecurrence>,
        PlannedPaymentId,
    ) {
        let payment = PlannedPayment {
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
                    from: day(1, 1),
                    value: Money::new(Decimal::from(700), Currency::Eur),
                    margin: Money::zero(Currency::Eur),
                }],
                revision: None,
                observed: false,
            },
            window: Window::default(),
        };
        let id = payment.id.clone();
        let provider = Arc::new(InMemoryRecurrence::with([payment]));
        (
            SetOccurrenceException {
                recurrence_provider: provider.clone(),
            },
            provider,
            id,
        )
    }

    fn command(id: &PlannedPaymentId, date: NaiveDate, request: Request) -> Command {
        Command {
            payment_id: id.clone(),
            date,
            request,
        }
    }

    #[tokio::test]
    async fn saute_change_puis_retire_une_exception() {
        let (use_case, provider, id) = setup();
        use_case
            .handle(command(&id, day(11, 5), Request::Skip))
            .await
            .unwrap();
        assert_eq!(
            provider.read_exceptions().await.unwrap()[0].2,
            ExceptionAction::Skip
        );

        // Une seconde exception sur la même échéance remplace la première.
        use_case
            .handle(command(
                &id,
                day(11, 5),
                Request::Amount(Decimal::from(250)),
            ))
            .await
            .unwrap();
        let exceptions = provider.read_exceptions().await.unwrap();
        assert_eq!(exceptions.len(), 1);
        assert_eq!(
            exceptions[0].2,
            ExceptionAction::Amount(Money::new(Decimal::from(250), Currency::Eur))
        );

        use_case
            .handle(command(&id, day(11, 5), Request::Clear))
            .await
            .unwrap();
        assert!(provider.read_exceptions().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn refuse_une_date_sans_echeance_un_montant_negatif_ou_un_paiement_inconnu() {
        let (use_case, provider, id) = setup();
        // Le 6 n'est pas une échéance (le rythme donne le 5).
        assert!(matches!(
            use_case
                .handle(command(&id, day(11, 6), Request::Skip))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(matches!(
            use_case
                .handle(command(&id, day(11, 5), Request::Amount(Decimal::from(-1))))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(matches!(
            use_case
                .handle(command(
                    &PlannedPaymentId::generate(),
                    day(11, 5),
                    Request::Skip
                ))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(provider.read_exceptions().await.unwrap().is_empty());
    }
}
