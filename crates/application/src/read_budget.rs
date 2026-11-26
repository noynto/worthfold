use std::sync::Arc;

use asset::{Asset, AssetKind, AssetProvider};
use budget::{BudgetProvider, SavingsPlan, month_start};
use category::{Category, CategoryKind, CategoryProvider};
use chrono::NaiveDate;
use service::budget::{
    Cushion, EnvelopeStatus, MonthSummary, SavingsSuggestion, envelope_statuses, month_summary,
    recommended_cushion, savings_suggestion,
};
use shared::{Currency, Money};
use transaction::TransactionProvider;
use valuation::ValuationProvider;

use crate::UseCaseError;

pub struct ReadBudget {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
    pub budget_provider: Arc<dyn BudgetProvider>,
}

pub struct Query {
    /// Le mois à regarder (n'importe quel jour du mois).
    pub month: NaiveDate,
    /// La date du jour, fournie par l'appelant : soldes du moment et mois complets du matelas.
    pub today: NaiveDate,
    pub currency: Currency,
}

pub struct Output {
    /// Le premier jour du mois regardé.
    pub month: NaiveDate,
    pub summary: MonthSummary,
    /// Une ligne par catégorie de dépense qui a un plafond ce mois-là.
    pub envelopes: Vec<EnvelopeStatus>,
    /// Les catégories de dépense, pour en fixer le plafond.
    pub expense_categories: Vec<Category>,
    /// Les livrets d'épargne, pour choisir la destination.
    pub savings_books: Vec<Asset>,
    pub plan: Option<SavingsPlan>,
    /// Absent tant que le réglage de l'épargne n'est pas fait.
    pub savings: Option<SavingsReport>,
}

pub struct SavingsReport {
    pub destination: Asset,
    /// Ce que contiennent les comptes courants aujourd'hui.
    pub current_balance: Money,
    pub destination_balance: Money,
    /// Le matelas évalué d'après les dépenses réelles, quand l'historique le permet.
    pub evaluated_cushion: Option<Cushion>,
    /// Le matelas retenu : celui fixé à la main, sinon celui évalué. Absent si aucun des deux.
    pub cushion: Option<Money>,
    /// Absent tant que le matelas n'est pas connu.
    pub suggestion: Option<SavingsSuggestion>,
}

impl ReadBudget {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let month = month_start(query.month);
        let currency = query.currency;
        let assets = self.asset_provider.read_all().await?;
        let categories = self.category_provider.read_all().await?;
        let transactions = self.transaction_provider.read_all().await?;
        let envelopes = self.budget_provider.read_envelopes().await?;
        let plan = self.budget_provider.read_savings_plan().await?;

        let summary = month_summary(&transactions, &categories, month, currency);
        let statuses = envelope_statuses(&envelopes, &categories, &transactions, month, currency);
        let savings_books: Vec<Asset> = assets
            .iter()
            .filter(|a| a.kind == AssetKind::SavingsBook && a.currency == currency)
            .cloned()
            .collect();
        let expense_categories = categories
            .iter()
            .filter(|c| c.kind == CategoryKind::Expense)
            .cloned()
            .collect();

        let savings = match &plan {
            Some(plan) => {
                let destination = assets.iter().find(|a| a.id == plan.destination).cloned();
                match destination {
                    // Le livret a disparu sans que le réglage soit nettoyé : on le traite comme non réglé.
                    None => None,
                    Some(destination) => {
                        let valuations = self.valuation_provider.read_all().await?;
                        let balance = |asset: &Asset| {
                            service::latest_valuation(asset, &valuations, query.today)
                                .map_or(rust_decimal::Decimal::ZERO, |v| v.value.amount)
                        };
                        let current = assets
                            .iter()
                            .filter(|a| a.kind == AssetKind::BankAccount && a.currency == currency)
                            .map(balance)
                            .sum();
                        let current_balance = Money::new(current, currency);
                        let destination_balance = Money::new(balance(&destination), currency);
                        let evaluated_cushion =
                            recommended_cushion(&transactions, &categories, query.today, currency);
                        let cushion = plan
                            .cushion_override
                            .or(evaluated_cushion.map(|c| c.amount));
                        let suggestion = cushion.map(|cushion| {
                            savings_suggestion(
                                &summary,
                                current_balance,
                                cushion,
                                destination_balance,
                                plan.ceiling,
                            )
                        });
                        Some(SavingsReport {
                            destination,
                            current_balance,
                            destination_balance,
                            evaluated_cushion,
                            cushion,
                            suggestion,
                        })
                    }
                }
            }
            None => None,
        };
        Ok(Output {
            month,
            summary,
            envelopes: statuses,
            expense_categories,
            savings_books,
            plan,
            savings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use shared::{CategoryId, TransactionId, ValuationId};
    use testing::{
        InMemoryAssets, InMemoryBudget, InMemoryCategories, InMemoryTransactions,
        InMemoryValuations,
    };
    use transaction::{Transaction, TransactionSource};
    use valuation::Valuation;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn eur(amount: i64) -> Money {
        Money::new(Decimal::from(amount), Currency::Eur)
    }

    fn asset(id: &str, kind: AssetKind) -> Asset {
        Asset {
            id: id.into(),
            name: id.into(),
            kind,
            currency: Currency::Eur,
        }
    }

    fn category(id: &str, kind: CategoryKind) -> Category {
        Category {
            id: id.into(),
            name: id.into(),
            kind,
        }
    }

    fn transaction(date: NaiveDate, amount: i64, category: &str) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: "compte".into(),
            booked_on: date,
            amount: eur(amount),
            label: "x".into(),
            source: TransactionSource::Csv,
            category_id: Some(CategoryId::from(category)),
        }
    }

    fn valuation(asset: &str, amount: i64) -> Valuation {
        Valuation {
            id: ValuationId::generate(),
            asset_id: asset.into(),
            value: eur(amount),
            valued_on: day(2026, 11, 20),
        }
    }

    /// Octobre : 2 000 € de salaire, 1 500 € de dépenses. Le compte contient 5 000 €, le livret 1 000 €.
    fn use_case(plan: Option<SavingsPlan>) -> ReadBudget {
        let transactions = vec![
            transaction(day(2026, 10, 1), 2000, "salaire"),
            transaction(day(2026, 10, 5), -700, "loyer"),
            transaction(day(2026, 10, 28), -800, "courses"),
        ];
        ReadBudget {
            asset_provider: Arc::new(InMemoryAssets::with([
                asset("compte", AssetKind::BankAccount),
                asset("livret", AssetKind::SavingsBook),
            ])),
            valuation_provider: Arc::new(InMemoryValuations::with([
                valuation("compte", 5000),
                valuation("livret", 1000),
            ])),
            transaction_provider: Arc::new(InMemoryTransactions::with(transactions)),
            category_provider: Arc::new(InMemoryCategories::with(
                [
                    category("salaire", CategoryKind::Income),
                    category("loyer", CategoryKind::Expense),
                    category("courses", CategoryKind::Expense),
                ],
                [],
            )),
            budget_provider: Arc::new(InMemoryBudget::with([], plan)),
        }
    }

    fn query() -> Query {
        Query {
            month: day(2026, 10, 17),
            today: day(2026, 11, 20),
            currency: Currency::Eur,
        }
    }

    #[tokio::test]
    async fn sans_reglage_de_l_epargne_rend_la_synthese_et_les_livrets_a_choisir() {
        let output = use_case(None).handle(query()).await.unwrap();
        assert_eq!(output.month, day(2026, 10, 1));
        assert_eq!(output.summary.income, Decimal::from(2000));
        assert_eq!(output.summary.spending(), Decimal::from(1500));
        assert_eq!(output.savings_books.len(), 1);
        assert_eq!(output.expense_categories.len(), 2);
        assert!(output.savings.is_none());
    }

    #[tokio::test]
    async fn evalue_le_matelas_puis_suggere_le_versement() {
        let plan = SavingsPlan {
            destination: "livret".into(),
            ceiling: eur(22_950),
            cushion_override: None,
        };
        let output = use_case(Some(plan)).handle(query()).await.unwrap();
        let savings = output.savings.unwrap();

        assert_eq!(savings.current_balance, eur(5000));
        assert_eq!(savings.destination_balance, eur(1000));
        // Octobre est le seul mois complet : 1 500 € de dépenses.
        let evaluated = savings.evaluated_cushion.unwrap();
        assert_eq!((evaluated.amount, evaluated.months), (eur(1500), 1));
        // 500 € de reste sur le mois, 3 500 € au-dessus du matelas : on verse 500 €.
        assert_eq!(savings.suggestion.unwrap().amount, eur(500));
    }

    #[tokio::test]
    async fn un_matelas_fixe_a_la_main_remplace_celui_evalue() {
        let plan = SavingsPlan {
            destination: "livret".into(),
            ceiling: eur(22_950),
            cushion_override: Some(eur(4800)),
        };
        let savings = use_case(Some(plan))
            .handle(query())
            .await
            .unwrap()
            .savings
            .unwrap();
        assert_eq!(savings.cushion, Some(eur(4800)));
        // Seulement 200 € au-dessus d'un matelas de 4 800 €.
        assert_eq!(savings.suggestion.unwrap().amount, eur(200));
    }

    #[tokio::test]
    async fn sans_historique_complet_le_matelas_est_inconnu_et_rien_n_est_suggere() {
        let plan = SavingsPlan {
            destination: "livret".into(),
            ceiling: eur(22_950),
            cushion_override: None,
        };
        // Nous sommes en octobre : aucun mois complet n'est encore passé.
        let mut early = query();
        early.today = day(2026, 10, 20);
        let savings = use_case(Some(plan))
            .handle(early)
            .await
            .unwrap()
            .savings
            .unwrap();
        assert!(savings.cushion.is_none() && savings.suggestion.is_none());
    }

    #[tokio::test]
    async fn un_reglage_dont_le_livret_a_disparu_est_traite_comme_non_regle() {
        let plan = SavingsPlan {
            destination: "disparu".into(),
            ceiling: eur(22_950),
            cushion_override: None,
        };
        let output = use_case(Some(plan)).handle(query()).await.unwrap();
        assert!(output.savings.is_none());
    }
}
