//! Les tâches one-shot (voir docs/tech/tasks.md) : chacune appelle un cas d'usage et journalise son compte rendu.

use std::process::ExitCode;

use application::check_session::{self, SessionState};
use application::{UseCases, sync_bank_accounts};

/// Lit les opérations de tous les comptes bancaires rattachés. Code 1 si un compte ou un consentement
/// est en échec, pour que le planificateur le signale.
pub async fn bank_sync(use_cases: &UseCases) -> ExitCode {
    let today = chrono::Local::now().date_naive();
    match use_cases
        .sync_bank_accounts
        .handle(sync_bank_accounts::Command { today, full: false })
        .await
    {
        Ok(output) => {
            for account in &output.accounts {
                match &account.failure {
                    None => tracing::info!(
                        compte = %account.label,
                        nouvelles = account.imported,
                        connues = account.already_known,
                        "Opérations lues."
                    ),
                    Some(reason) => {
                        tracing::error!(compte = %account.label, %reason, "Lecture en échec.");
                    }
                }
            }
            for bank in &output.expired {
                tracing::error!(banque = %bank, "Consentement expiré : à renouveler depuis la page Banque.");
            }
            if output.unlinked > 0 {
                tracing::warn!(
                    comptes = output.unlinked,
                    "Comptes de la banque non rattachés."
                );
            }
            if output.succeeded() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            tracing::error!(error = ?e, "Synchronisation bancaire impossible.");
            ExitCode::FAILURE
        }
    }
}

/// Mot de passe oublié : retire le mot de passe et toutes les sessions. Au prochain démarrage du serveur,
/// la page de première configuration redemande un mot de passe, avec un nouveau code affiché dans la console.
pub async fn reset_password(use_cases: &UseCases) -> ExitCode {
    match use_cases.reset_password.handle().await {
        Ok(()) => {
            tracing::info!("Mot de passe retiré : relance worthfold pour en choisir un nouveau.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(error = ?e, "Réinitialisation du mot de passe impossible.");
            ExitCode::FAILURE
        }
    }
}

/// Tant qu'aucun mot de passe n'existe, dit où le choisir et affiche le code de configuration. Le code n'est
/// affiché que dans cette console : y avoir accès prouve qu'on a la main sur la machine.
pub async fn announce_setup(use_cases: &UseCases, setup_code: &str) {
    let state = use_cases
        .check_session
        .handle(check_session::Query {
            token: None,
            now: chrono::Utc::now(),
        })
        .await;
    if matches!(state, Ok(SessionState::NotConfigured)) {
        // Pas d'adresse complète : dans un conteneur, le port publié n'est pas celui d'écoute.
        tracing::warn!(
            "Première configuration : ouvre la page /setup de worthfold et saisis le code {setup_code}"
        );
    }
}
