use std::sync::Arc;

use chrono::NaiveDate;
use recurrence::{AmountStep, PlannedPayment, RecurrenceProvider};
use rust_decimal::Decimal;
use shared::{Money, PlannedPaymentId};

use crate::UseCaseError;
use crate::delete_planned_payment::PAYMENT_NOT_FOUND;

pub struct AddAmountStep {
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Command {
    pub payment_id: PlannedPaymentId,
    /// La date à partir de laquelle le nouveau montant s'applique.
    pub from: NaiveDate,
    /// Le nouveau montant, en positif.
    pub value: Decimal,
    /// L'incertitude autour du montant ; zéro pour un montant exact (le vrai montant, une fois connu).
    pub margin: Decimal,
}

pub struct Output {
    pub payment: PlannedPayment,
}

impl AddAmountStep {
    /// Ajoute un palier de montant : une révision connue, l'avis d'imposition qui remplace l'estimation…
    /// Les échéances d'avant `from` gardent leur ancien montant (l'historique ne s'écrase jamais) ;
    /// un second palier à la **même date** corrige le premier.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let mut payment = self
            .recurrence_provider
            .read(&command.payment_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(PAYMENT_NOT_FOUND))?;
        let currency = payment.amount.steps[0].value.currency;
        payment.amount.steps.retain(|s| s.from != command.from);
        payment.amount.steps.push(AmountStep {
            from: command.from,
            value: Money::new(command.value, currency),
            margin: Money::new(command.margin, currency),
        });
        payment.amount = payment.amount.validate().map_err(UseCaseError::refusal)?;
        self.recurrence_provider.write(&payment).await?;
        Ok(Output { payment })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recurrence::{Amount, DayOfMonth, Direction, Schedule, Window};
    use shared::Currency;
    use testing::InMemoryRecurrence;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn setup() -> (AddAmountStep, Arc<InMemoryRecurrence>, PlannedPaymentId) {
        let eur = |a: i64| Money::new(Decimal::from(a), Currency::Eur);
        let payment = PlannedPayment {
            id: PlannedPaymentId::generate(),
            label: "Fermage".into(),
            pattern: "fermage".into(),
            direction: Direction::In,
            account_id: "compte".into(),
            category_id: None,
            schedule: Schedule::Monthly {
                day: DayOfMonth::Day(10),
            },
            amount: Amount {
                steps: vec![AmountStep {
                    from: day(2026, 1, 1),
                    value: eur(4000),
                    margin: eur(0),
                }],
                revision: None,
                observed: false,
            },
            window: Window::default(),
        };
        let id = payment.id.clone();
        let provider = Arc::new(InMemoryRecurrence::with([payment]));
        (
            AddAmountStep {
                recurrence_provider: provider.clone(),
            },
            provider,
            id,
        )
    }

    fn command(id: &PlannedPaymentId, from: NaiveDate, value: i64, margin: i64) -> Command {
        Command {
            payment_id: id.clone(),
            from,
            value: Decimal::from(value),
            margin: Decimal::from(margin),
        }
    }

    #[tokio::test]
    async fn ajoute_un_palier_et_garde_les_anciens() {
        let (use_case, provider, id) = setup();
        let output = use_case
            .handle(command(&id, day(2026, 10, 15), 4080, 0))
            .await
            .unwrap();
        assert_eq!(output.payment.amount.steps.len(), 2);
        let stored = provider.read(&id).await.unwrap().unwrap();
        assert_eq!(stored.amount.steps[0].value.amount, Decimal::from(4000));
        assert_eq!(stored.amount.steps[1].value.amount, Decimal::from(4080));
        // Les échéances d'avant le palier gardent l'ancien montant.
        assert_eq!(
            stored
                .amount
                .expected_at(day(2026, 9, 10), None)
                .value
                .amount,
            Decimal::from(4000)
        );
    }

    #[tokio::test]
    async fn un_palier_a_la_meme_date_corrige_le_precedent() {
        let (use_case, _, id) = setup();
        use_case
            .handle(command(&id, day(2026, 10, 15), 4080, 0))
            .await
            .unwrap();
        let output = use_case
            .handle(command(&id, day(2026, 10, 15), 4100, 0))
            .await
            .unwrap();
        assert_eq!(output.payment.amount.steps.len(), 2);
        assert_eq!(
            output.payment.amount.steps[1].value.amount,
            Decimal::from(4100)
        );
    }

    #[tokio::test]
    async fn refuse_un_montant_negatif_et_un_paiement_inconnu() {
        let (use_case, provider, id) = setup();
        assert!(matches!(
            use_case
                .handle(command(&id, day(2026, 10, 15), -5, 0))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(matches!(
            use_case
                .handle(command(&id, day(2026, 10, 15), 5, -1))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert_eq!(
            provider
                .read(&id)
                .await
                .unwrap()
                .unwrap()
                .amount
                .steps
                .len(),
            1
        );
        assert!(matches!(
            use_case
                .handle(command(
                    &PlannedPaymentId::generate(),
                    day(2026, 10, 15),
                    5,
                    0
                ))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
