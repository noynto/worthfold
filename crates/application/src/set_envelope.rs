use std::sync::Arc;

use budget::{BudgetProvider, Envelope, effective_limit, month_start};
use category::{Category, CategoryKind, CategoryProvider};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{CategoryId, Currency, EnvelopeId, Money};

use crate::{CATEGORY_NOT_FOUND, UseCaseError};

pub struct SetEnvelope {
    pub category_provider: Arc<dyn CategoryProvider>,
    pub budget_provider: Arc<dyn BudgetProvider>,
}

pub struct Command {
    pub category_id: CategoryId,
    /// Le plafond mensuel, ou `None` pour ne plus avoir d'enveloppe pour cette catégorie.
    pub limit: Option<Decimal>,
    pub currency: Currency,
    /// Le mois à partir duquel le plafond s'applique (n'importe quel jour du mois).
    /// Les mois précédents gardent leur plafond.
    pub from_month: NaiveDate,
}

pub struct Output {
    pub category: Category,
    pub envelope: Envelope,
}

impl SetEnvelope {
    /// Fixe ou retire le plafond d'une catégorie à partir d'un mois. Une ligne est **ajoutée** :
    /// l'historique des plafonds ne s'écrase jamais.
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let category = self
            .category_provider
            .read(&command.category_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(CATEGORY_NOT_FOUND))?;
        if category.kind != CategoryKind::Expense {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'est pas une catégorie de dépense : seules les dépenses ont une enveloppe.",
                category.name
            )));
        }
        let from_month = month_start(command.from_month);
        let limit = match command.limit {
            Some(amount) if amount.is_sign_negative() && !amount.is_zero() => {
                return Err(UseCaseError::refusal(
                    "Le plafond ne peut pas être négatif.",
                ));
            }
            Some(amount) => Some(Money::new(amount, command.currency)),
            None => {
                // Retirer une enveloppe qui n'existe pas n'aurait aucun effet : on le dit.
                let envelopes = self.budget_provider.read_envelopes().await?;
                if effective_limit(&envelopes, &category.id, from_month).is_none() {
                    return Err(UseCaseError::refusal(format!(
                        "« {} » n'a pas d'enveloppe à retirer ce mois-là.",
                        category.name
                    )));
                }
                None
            }
        };
        let envelope = Envelope {
            id: EnvelopeId::generate(),
            category_id: category.id.clone(),
            limit,
            from_month,
        };
        self.budget_provider.write_envelope(&envelope).await?;
        Ok(Output { category, envelope })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{InMemoryBudget, InMemoryCategories};

    fn category(id: &str, kind: CategoryKind) -> Category {
        Category {
            id: id.into(),
            name: id.into(),
            kind,
        }
    }

    fn setup() -> (SetEnvelope, Arc<InMemoryBudget>) {
        let budget = Arc::new(InMemoryBudget::default());
        (
            SetEnvelope {
                category_provider: Arc::new(InMemoryCategories::with(
                    [
                        category("courses", CategoryKind::Expense),
                        category("salaire", CategoryKind::Income),
                    ],
                    [],
                )),
                budget_provider: budget.clone(),
            },
            budget,
        )
    }

    fn command(category: &str, limit: Option<i64>, month: u32, day: u32) -> Command {
        Command {
            category_id: category.into(),
            limit: limit.map(Decimal::from),
            currency: Currency::Eur,
            from_month: NaiveDate::from_ymd_opt(2026, month, day).unwrap(),
        }
    }

    #[tokio::test]
    async fn ajoute_le_plafond_a_partir_du_premier_jour_du_mois() {
        let (use_case, budget) = setup();
        let output = use_case
            .handle(command("courses", Some(400), 11, 17))
            .await
            .unwrap();
        assert_eq!(
            output.envelope.from_month,
            NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
        );
        assert_eq!(
            output.envelope.limit,
            Some(Money::new(Decimal::from(400), Currency::Eur))
        );
        assert_eq!(budget.read_envelopes().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn changer_un_plafond_ajoute_une_ligne_et_garde_l_ancienne() {
        let (use_case, budget) = setup();
        use_case
            .handle(command("courses", Some(400), 1, 1))
            .await
            .unwrap();
        use_case
            .handle(command("courses", Some(500), 6, 1))
            .await
            .unwrap();
        assert_eq!(budget.read_envelopes().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn retire_une_enveloppe_existante_et_refuse_d_en_retirer_une_absente() {
        let (use_case, budget) = setup();
        assert!(matches!(
            use_case.handle(command("courses", None, 11, 1)).await,
            Err(UseCaseError::Refusal(_))
        ));
        use_case
            .handle(command("courses", Some(400), 1, 1))
            .await
            .unwrap();
        let output = use_case
            .handle(command("courses", None, 11, 1))
            .await
            .unwrap();
        assert_eq!(output.envelope.limit, None);
        assert_eq!(budget.read_envelopes().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn refuse_une_categorie_de_revenu_inconnue_ou_un_plafond_negatif() {
        let (use_case, budget) = setup();
        for refused in [
            command("salaire", Some(400), 11, 1),
            command("inconnue", Some(400), 11, 1),
            command("courses", Some(-1), 11, 1),
        ] {
            assert!(matches!(
                use_case.handle(refused).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert!(budget.read_envelopes().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accepte_un_plafond_a_zero() {
        let (use_case, _) = setup();
        assert!(
            use_case
                .handle(command("courses", Some(0), 11, 1))
                .await
                .is_ok()
        );
    }
}
