//! Cas d'usage : un module par action, avec sa `Command` (ou `Query`), son `Output`
//! et une méthode `handle`. Seuls les cas d'usage parlent aux providers.

pub mod accept_recurrence_suggestion;
pub mod add_amount_step;
pub mod add_asset;
pub mod add_category;
pub mod add_category_rule;
pub mod add_default_categories;
pub mod add_institution;
pub mod add_planned_payment;
pub mod apply_category_rules;
mod bank_details;
pub mod categorize_transaction;
pub mod change_password;
pub mod check_session;
pub mod complete_bank_connection;
pub mod delete_asset;
pub mod delete_bank_connection;
pub mod delete_category;
pub mod delete_category_rule;
pub mod delete_institution;
pub mod delete_planned_payment;
pub mod delete_valuation;
pub mod import_transactions;
mod institution_format;
pub mod link_bank_account;
pub mod list_banks;
pub mod login;
pub mod logout;
pub mod read_asset;
pub mod read_bank_connections;
pub mod read_budget;
pub mod read_categories;
pub mod read_forecast;
pub mod read_institutions;
pub mod read_net_worth;
pub mod read_net_worth_history;
pub mod read_planned_payments;
pub mod read_transactions;
pub mod record_valuation;
pub mod reset_password;
pub mod save_savings_plan;
mod session;
pub mod set_envelope;
pub mod set_occurrence_exception;
pub mod setup_password;
pub mod start_bank_connection;
pub mod sync_bank_accounts;
pub mod update_asset;
pub mod update_institution;

use std::sync::Arc;

pub use auth::{MIN_PASSWORD_LENGTH, SESSION_DAYS};
pub use institution::{
    AmountColumns, CsvFormat, DateFormat, DaySeparator, DecimalSeparator, Delimiter, Encoding,
    PdfFormat,
};
pub use institution_format::FormatInput;
/// Les types de budget que les cas d'usage rendent dans leurs sorties : l'adaptateur web les lit
/// ici, sans dépendre de la crate `service` (voir docs/tech/architecture.md).
pub use service::budget::{
    Cushion, EnvelopeState, EnvelopeStatus, MonthSummary, SavingsLimit, SavingsSuggestion,
};
pub use service::forecast::{Forecast, ForecastMonth, HORIZON_MONTHS, Scenarios, VariableSpending};
pub use service::recurrence::{CalendarLine, OccurrenceStatus, ScheduleLine, Suggestion};
pub use transaction::TransactionSource;

use asset::AssetProvider;
use auth::{AuthProvider, PasswordHasher, Throttle};
use bank::{BankAggregator, BankConnectionProvider};
use budget::BudgetProvider;
use category::CategoryProvider;
use institution::{InstitutionProvider, PdfReader};
use recurrence::RecurrenceProvider;
use transaction::TransactionProvider;
use valuation::ValuationProvider;

/// Tous les cas d'usage, assemblés par `bootstrap` avec les adaptateurs de l'infrastructure, puis donnés
/// aux points d'entrée (serveur web, tâches). Ne contient rien de technique : seulement des cas d'usage.
pub struct UseCases {
    pub add_asset: add_asset::AddAsset,
    pub read_asset: read_asset::ReadAsset,
    pub update_asset: update_asset::UpdateAsset,
    pub delete_asset: delete_asset::DeleteAsset,
    pub record_valuation: record_valuation::RecordValuation,
    pub delete_valuation: delete_valuation::DeleteValuation,
    pub read_net_worth: read_net_worth::ReadNetWorth,
    pub read_net_worth_history: read_net_worth_history::ReadNetWorthHistory,
    pub import_transactions: import_transactions::ImportTransactions,
    pub read_transactions: read_transactions::ReadTransactions,
    pub read_categories: read_categories::ReadCategories,
    pub read_forecast: read_forecast::ReadForecast,
    pub read_institutions: read_institutions::ReadInstitutions,
    pub add_institution: add_institution::AddInstitution,
    pub update_institution: update_institution::UpdateInstitution,
    pub delete_institution: delete_institution::DeleteInstitution,
    pub add_category: add_category::AddCategory,
    pub add_default_categories: add_default_categories::AddDefaultCategories,
    pub delete_category: delete_category::DeleteCategory,
    pub add_category_rule: add_category_rule::AddCategoryRule,
    pub delete_category_rule: delete_category_rule::DeleteCategoryRule,
    pub apply_category_rules: apply_category_rules::ApplyCategoryRules,
    pub categorize_transaction: categorize_transaction::CategorizeTransaction,
    pub read_budget: read_budget::ReadBudget,
    pub set_envelope: set_envelope::SetEnvelope,
    pub save_savings_plan: save_savings_plan::SaveSavingsPlan,
    pub read_planned_payments: read_planned_payments::ReadPlannedPayments,
    pub add_planned_payment: add_planned_payment::AddPlannedPayment,
    pub delete_planned_payment: delete_planned_payment::DeletePlannedPayment,
    pub add_amount_step: add_amount_step::AddAmountStep,
    pub set_occurrence_exception: set_occurrence_exception::SetOccurrenceException,
    pub accept_recurrence_suggestion: accept_recurrence_suggestion::AcceptRecurrenceSuggestion,
    pub list_banks: list_banks::ListBanks,
    pub read_bank_connections: read_bank_connections::ReadBankConnections,
    pub start_bank_connection: start_bank_connection::StartBankConnection,
    pub complete_bank_connection: complete_bank_connection::CompleteBankConnection,
    pub link_bank_account: link_bank_account::LinkBankAccount,
    pub sync_bank_accounts: sync_bank_accounts::SyncBankAccounts,
    pub delete_bank_connection: delete_bank_connection::DeleteBankConnection,
    pub setup_password: setup_password::SetupPassword,
    pub login: login::Login,
    pub logout: logout::Logout,
    pub check_session: check_session::CheckSession,
    pub change_password: change_password::ChangePassword,
    pub reset_password: reset_password::ResetPassword,
}

/// Ce que l'authentification demande à l'extérieur : où ranger le mot de passe et les sessions, comment
/// calculer une empreinte, et le code de configuration tiré au démarrage.
pub struct AuthPorts {
    pub provider: Arc<dyn AuthProvider>,
    pub hasher: Arc<dyn PasswordHasher>,
    /// Affiché dans la console du serveur : sert à choisir le premier mot de passe.
    pub setup_code: String,
}

impl UseCases {
    /// Branche chaque cas d'usage sur les providers dont il a besoin : les adaptateurs SQLite en
    /// production (`bootstrap`), ceux en mémoire dans les tests.
    // Un paramètre par provider : les regrouper dans une structure ne ferait que déplacer la liste.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        assets: Arc<dyn AssetProvider>,
        valuations: Arc<dyn ValuationProvider>,
        transactions: Arc<dyn TransactionProvider>,
        categories: Arc<dyn CategoryProvider>,
        budget: Arc<dyn BudgetProvider>,
        recurrence: Arc<dyn RecurrenceProvider>,
        bank: Arc<dyn BankConnectionProvider>,
        institutions: Arc<dyn InstitutionProvider>,
        pdf_reader: Arc<dyn PdfReader>,
        aggregator: Option<Arc<dyn BankAggregator>>,
        auth: AuthPorts,
    ) -> Self {
        // Un seul compteur d'essais pour toutes les saisies de mot de passe (configuration, connexion, changement).
        let throttle = Arc::new(Throttle::default());
        Self {
            add_asset: add_asset::AddAsset {
                asset_provider: assets.clone(),
                institution_provider: institutions.clone(),
            },
            read_asset: read_asset::ReadAsset {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
                institution_provider: institutions.clone(),
            },
            update_asset: update_asset::UpdateAsset {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
                institution_provider: institutions.clone(),
            },
            delete_asset: delete_asset::DeleteAsset {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
                transaction_provider: transactions.clone(),
                budget_provider: budget.clone(),
                recurrence_provider: recurrence.clone(),
                bank_provider: bank.clone(),
            },
            record_valuation: record_valuation::RecordValuation {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            delete_valuation: delete_valuation::DeleteValuation {
                valuation_provider: valuations.clone(),
            },
            read_net_worth: read_net_worth::ReadNetWorth {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
                institution_provider: institutions.clone(),
            },
            read_institutions: read_institutions::ReadInstitutions {
                institution_provider: institutions.clone(),
                asset_provider: assets.clone(),
            },
            add_institution: add_institution::AddInstitution {
                institution_provider: institutions.clone(),
            },
            update_institution: update_institution::UpdateInstitution {
                institution_provider: institutions.clone(),
            },
            delete_institution: delete_institution::DeleteInstitution {
                institution_provider: institutions.clone(),
                asset_provider: assets.clone(),
            },
            read_net_worth_history: read_net_worth_history::ReadNetWorthHistory {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
            },
            import_transactions: import_transactions::ImportTransactions {
                asset_provider: assets.clone(),
                transaction_provider: transactions.clone(),
                valuation_provider: valuations.clone(),
                category_provider: categories.clone(),
                institution_provider: institutions,
                pdf_reader,
            },
            read_transactions: read_transactions::ReadTransactions {
                asset_provider: assets.clone(),
                transaction_provider: transactions.clone(),
                category_provider: categories.clone(),
            },
            read_forecast: read_forecast::ReadForecast {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
                transaction_provider: transactions.clone(),
                category_provider: categories.clone(),
                budget_provider: budget.clone(),
                recurrence_provider: recurrence.clone(),
            },
            read_categories: read_categories::ReadCategories {
                category_provider: categories.clone(),
            },
            add_category: add_category::AddCategory {
                category_provider: categories.clone(),
            },
            add_default_categories: add_default_categories::AddDefaultCategories {
                category_provider: categories.clone(),
            },
            delete_category: delete_category::DeleteCategory {
                category_provider: categories.clone(),
                transaction_provider: transactions.clone(),
                budget_provider: budget.clone(),
                recurrence_provider: recurrence.clone(),
            },
            add_category_rule: add_category_rule::AddCategoryRule {
                category_provider: categories.clone(),
            },
            delete_category_rule: delete_category_rule::DeleteCategoryRule {
                category_provider: categories.clone(),
            },
            apply_category_rules: apply_category_rules::ApplyCategoryRules {
                category_provider: categories.clone(),
                transaction_provider: transactions.clone(),
            },
            categorize_transaction: categorize_transaction::CategorizeTransaction {
                category_provider: categories.clone(),
                transaction_provider: transactions.clone(),
            },
            read_budget: read_budget::ReadBudget {
                asset_provider: assets.clone(),
                valuation_provider: valuations.clone(),
                transaction_provider: transactions.clone(),
                category_provider: categories.clone(),
                budget_provider: budget.clone(),
            },
            set_envelope: set_envelope::SetEnvelope {
                category_provider: categories.clone(),
                budget_provider: budget.clone(),
            },
            save_savings_plan: save_savings_plan::SaveSavingsPlan {
                asset_provider: assets.clone(),
                budget_provider: budget,
            },
            read_planned_payments: read_planned_payments::ReadPlannedPayments {
                asset_provider: assets.clone(),
                category_provider: categories.clone(),
                transaction_provider: transactions.clone(),
                recurrence_provider: recurrence.clone(),
            },
            add_planned_payment: add_planned_payment::AddPlannedPayment {
                asset_provider: assets.clone(),
                category_provider: categories.clone(),
                recurrence_provider: recurrence.clone(),
            },
            delete_planned_payment: delete_planned_payment::DeletePlannedPayment {
                recurrence_provider: recurrence.clone(),
            },
            add_amount_step: add_amount_step::AddAmountStep {
                recurrence_provider: recurrence.clone(),
            },
            set_occurrence_exception: set_occurrence_exception::SetOccurrenceException {
                recurrence_provider: recurrence.clone(),
            },
            accept_recurrence_suggestion:
                accept_recurrence_suggestion::AcceptRecurrenceSuggestion {
                    category_provider: categories.clone(),
                    transaction_provider: transactions.clone(),
                    recurrence_provider: recurrence,
                },
            list_banks: list_banks::ListBanks {
                aggregator: aggregator.clone(),
            },
            read_bank_connections: read_bank_connections::ReadBankConnections {
                configured: aggregator.is_some(),
                bank_provider: bank.clone(),
                asset_provider: assets.clone(),
            },
            start_bank_connection: start_bank_connection::StartBankConnection {
                aggregator: aggregator.clone(),
                bank_provider: bank.clone(),
            },
            complete_bank_connection: complete_bank_connection::CompleteBankConnection {
                aggregator: aggregator.clone(),
                bank_provider: bank.clone(),
            },
            link_bank_account: link_bank_account::LinkBankAccount {
                asset_provider: assets.clone(),
                bank_provider: bank.clone(),
            },
            sync_bank_accounts: sync_bank_accounts::SyncBankAccounts {
                aggregator: aggregator.clone(),
                bank_provider: bank.clone(),
                asset_provider: assets.clone(),
                transaction_provider: transactions.clone(),
                valuation_provider: valuations.clone(),
                category_provider: categories.clone(),
            },
            delete_bank_connection: delete_bank_connection::DeleteBankConnection {
                aggregator,
                bank_provider: bank,
            },
            setup_password: setup_password::SetupPassword {
                auth_provider: auth.provider.clone(),
                hasher: auth.hasher.clone(),
                throttle: throttle.clone(),
                setup_code: auth.setup_code.into(),
            },
            login: login::Login {
                auth_provider: auth.provider.clone(),
                hasher: auth.hasher.clone(),
                throttle: throttle.clone(),
            },
            logout: logout::Logout {
                auth_provider: auth.provider.clone(),
            },
            check_session: check_session::CheckSession {
                auth_provider: auth.provider.clone(),
            },
            change_password: change_password::ChangePassword {
                auth_provider: auth.provider.clone(),
                hasher: auth.hasher,
                throttle,
            },
            reset_password: reset_password::ResetPassword {
                auth_provider: auth.provider,
            },
        }
    }
}

/// Erreur commune des cas d'usage.
#[derive(Debug, thiserror::Error)]
pub enum UseCaseError {
    /// Refus d'une règle métier : le message, en français, peut être montré tel quel.
    #[error("{0}")]
    Refusal(String),
    /// Panne technique (base, réseau…) : journalisée, jamais montrée en détail.
    #[error(transparent)]
    Technical(#[from] anyhow::Error),
}

/// Refus commun quand l'actif visé n'existe pas (lien périmé, actif supprimé entre-temps).
/// Jamais d'identifiant technique dans un message montré à l'utilisateur.
/// Refus commun quand la connexion bancaire n'est pas configurée (identifiant d'application et clé absents).
pub(crate) const BANK_NOT_CONFIGURED: &str = "La connexion bancaire n'est pas configurée : renseigne WORTHFOLD_ENABLEBANKING_APP_ID et WORTHFOLD_ENABLEBANKING_KEY_PATH, puis relance worthfold.";

pub(crate) const NOT_FOUND: &str = "Cet actif n'existe plus : il a peut-être été supprimé.";

/// Refus commun quand la catégorie visée n'existe pas (lien périmé, catégorie supprimée entre-temps).
pub(crate) const CATEGORY_NOT_FOUND: &str =
    "Cette catégorie n'existe plus : elle a peut-être été supprimée.";

impl UseCaseError {
    pub fn refusal(message: impl Into<String>) -> Self {
        Self::Refusal(message.into())
    }
}
