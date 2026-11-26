use std::sync::Arc;

use asset::{AssetKind, AssetProvider};
use category::{CategoryKind, CategoryProvider, normalize};
use recurrence::{Amount, Direction, PlannedPayment, RecurrenceProvider, Schedule, Window};
use shared::{AssetId, CategoryId, PlannedPaymentId};

use crate::{CATEGORY_NOT_FOUND, NOT_FOUND, UseCaseError};

pub struct AddPlannedPayment {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
    pub recurrence_provider: Arc<dyn RecurrenceProvider>,
}

pub struct Command {
    pub label: String,
    /// Le mot du libellé bancaire qui reconnaît ce paiement dans les relevés.
    pub pattern: String,
    pub direction: Direction,
    pub account_id: AssetId,
    pub category_id: Option<CategoryId>,
    pub schedule: Schedule,
    pub amount: Amount,
    pub window: Window,
}

pub struct Output {
    pub payment: PlannedPayment,
}

impl AddPlannedPayment {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let label =
            PlannedPayment::validate_label(&command.label).map_err(UseCaseError::refusal)?;
        let pattern =
            PlannedPayment::validate_pattern(&command.pattern).map_err(UseCaseError::refusal)?;
        let schedule = command.schedule.validate().map_err(UseCaseError::refusal)?;
        let amount = command.amount.validate().map_err(UseCaseError::refusal)?;
        let window = command.window.validate().map_err(UseCaseError::refusal)?;

        let account = self
            .asset_provider
            .read(&command.account_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        if account.kind != AssetKind::BankAccount {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'est pas un compte bancaire : un paiement prévu se rattache à un compte bancaire.",
                account.name
            )));
        }
        if amount
            .steps
            .iter()
            .any(|s| s.value.currency != account.currency)
        {
            return Err(UseCaseError::refusal(format!(
                "Le montant doit être en {}, la devise de « {} ».",
                account.currency, account.name
            )));
        }
        if let Some(category_id) = &command.category_id {
            let category = self
                .category_provider
                .read(category_id)
                .await?
                .ok_or_else(|| UseCaseError::refusal(CATEGORY_NOT_FOUND))?;
            let compatible = match (command.direction, category.kind) {
                (_, CategoryKind::Transfer)
                | (Direction::In, CategoryKind::Income)
                | (Direction::Out, CategoryKind::Expense) => true,
                (Direction::In, CategoryKind::Expense) | (Direction::Out, CategoryKind::Income) => {
                    false
                }
            };
            if !compatible {
                return Err(UseCaseError::refusal(format!(
                    "« {} » est une catégorie de {} : elle ne va pas avec une {}.",
                    category.name,
                    category.kind.label().to_lowercase(),
                    command.direction.label().to_lowercase()
                )));
            }
        }
        // Deux paiements reconnus par le même mot, sur le même compte et dans le même sens,
        // se disputeraient les mêmes opérations.
        let existing = self.recurrence_provider.read_all().await?;
        if let Some(twin) = existing.iter().find(|p| {
            p.account_id == account.id
                && p.direction == command.direction
                && normalize(&p.pattern) == normalize(&pattern)
        }) {
            return Err(UseCaseError::refusal(format!(
                "Le mot « {pattern} » reconnaît déjà « {} » sur ce compte : change de mot ou supprime ce paiement.",
                twin.label
            )));
        }
        let payment = PlannedPayment {
            id: PlannedPaymentId::generate(),
            label,
            pattern,
            direction: command.direction,
            account_id: account.id,
            category_id: command.category_id,
            schedule,
            amount,
            window,
        };
        self.recurrence_provider.write(&payment).await?;
        Ok(Output { payment })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::Asset;
    use category::Category;
    use chrono::NaiveDate;
    use recurrence::{AmountStep, DayOfMonth};
    use rust_decimal::Decimal;
    use shared::{Currency, Money};
    use testing::{InMemoryAssets, InMemoryCategories, InMemoryRecurrence};

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn setup() -> (AddPlannedPayment, Arc<InMemoryRecurrence>) {
        let asset = |id: &str, kind| Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        let category = |id: &str, kind| Category {
            id: id.into(),
            name: id.into(),
            kind,
        };
        let provider = Arc::new(InMemoryRecurrence::default());
        (
            AddPlannedPayment {
                asset_provider: Arc::new(InMemoryAssets::with([
                    asset("compte", AssetKind::BankAccount),
                    asset("livret", AssetKind::SavingsBook),
                ])),
                category_provider: Arc::new(InMemoryCategories::with(
                    [
                        category("salaire", CategoryKind::Income),
                        category("courses", CategoryKind::Expense),
                        category("virement", CategoryKind::Transfer),
                    ],
                    [],
                )),
                recurrence_provider: provider.clone(),
            },
            provider,
        )
    }

    fn command() -> Command {
        Command {
            label: "  Salaire ".into(),
            pattern: " salaire ".into(),
            direction: Direction::In,
            account_id: "compte".into(),
            category_id: Some("salaire".into()),
            schedule: Schedule::Monthly {
                day: DayOfMonth::Day(28),
            },
            amount: Amount {
                steps: vec![AmountStep {
                    from: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    value: eur(1800),
                    margin: eur(150),
                }],
                revision: None,
                observed: true,
            },
            window: Window::default(),
        }
    }

    #[tokio::test]
    async fn ajoute_un_paiement_au_nom_et_au_mot_nettoyes() {
        let (use_case, provider) = setup();
        let output = use_case.handle(command()).await.unwrap();
        assert_eq!(output.payment.label, "Salaire");
        assert_eq!(output.payment.pattern, "salaire");
        assert_eq!(provider.read_all().await.unwrap(), vec![output.payment]);
    }

    #[tokio::test]
    async fn refuse_ce_que_le_domaine_refuse() {
        let (use_case, provider) = setup();
        let mut cases = Vec::new();
        let mut c = command();
        c.label = "  ".into();
        cases.push(c);
        let mut c = command();
        c.pattern = "".into();
        cases.push(c);
        let mut c = command();
        c.schedule = Schedule::Monthly {
            day: DayOfMonth::Day(40),
        };
        cases.push(c);
        let mut c = command();
        c.amount.steps.clear();
        cases.push(c);
        let mut c = command();
        c.window.occurrences = Some(3); // sans date de début
        cases.push(c);
        for case in cases {
            assert!(matches!(
                use_case.handle(case).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert!(provider.read_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn refuse_un_compte_inconnu_ou_qui_n_est_pas_bancaire() {
        let (use_case, _) = setup();
        for account in ["inconnu", "livret"] {
            let mut c = command();
            c.account_id = account.into();
            assert!(matches!(
                use_case.handle(c).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
    }

    #[tokio::test]
    async fn la_categorie_doit_aller_avec_le_sens() {
        let (use_case, _) = setup();
        // Une entrée d'argent dans une catégorie de dépense, ou l'inverse : refusé.
        let mut c = command();
        c.category_id = Some("courses".into());
        assert!(matches!(
            use_case.handle(c).await,
            Err(UseCaseError::Refusal(_))
        ));
        let mut c = command();
        c.direction = Direction::Out;
        assert!(matches!(
            use_case.handle(c).await,
            Err(UseCaseError::Refusal(_))
        ));
        // Un virement interne va avec les deux sens ; une catégorie inconnue est refusée.
        for direction in [Direction::In, Direction::Out] {
            let mut c = command();
            c.pattern = format!("vir {}", direction.code());
            c.direction = direction;
            c.category_id = Some("virement".into());
            assert!(use_case.handle(c).await.is_ok());
        }
        let mut c = command();
        c.category_id = Some("inconnue".into());
        assert!(matches!(
            use_case.handle(c).await,
            Err(UseCaseError::Refusal(_))
        ));
    }

    #[tokio::test]
    async fn refuse_deux_paiements_reconnus_par_le_meme_mot() {
        let (use_case, provider) = setup();
        use_case.handle(command()).await.unwrap();
        let mut twin = command();
        twin.label = "Autre".into();
        twin.pattern = "SALAIRE".into();
        assert!(matches!(
            use_case.handle(twin).await,
            Err(UseCaseError::Refusal(_))
        ));
        // Le même mot dans l'autre sens, c'est autre chose (un remboursement, par exemple).
        let mut other_direction = command();
        other_direction.direction = Direction::Out;
        other_direction.category_id = None;
        assert!(use_case.handle(other_direction).await.is_ok());
        assert_eq!(provider.read_all().await.unwrap().len(), 2);
    }
}
