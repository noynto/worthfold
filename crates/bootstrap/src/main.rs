//! Point d'entrée de worthfold (binaire `worthfold`) et composition root.
//!
//! Lit la configuration, assemble les adaptateurs et les cas d'usage, puis lance **soit une tâche**,
//! **soit le serveur web** : comme le `Bootstrap` d'EOSA, un seul binaire (et une seule image) sert aux
//! deux, et une variable d'environnement choisit la tâche (voir docs/tech/tasks.md).

mod assembly;
mod config;

use std::process::ExitCode;
use std::sync::Arc;

use config::Config;

#[tokio::main]
async fn main() -> ExitCode {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();
    match run().await {
        Ok(code) => code,
        Err(e) => {
            tracing::error!(error = ?e, "worthfold s'est arrêté sur une erreur.");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<ExitCode> {
    let config = Config::from_env()?;
    let use_cases = Arc::new(assembly::build(&config).await?);

    // Tâches one-shot : si la variable d'activation d'une tâche est définie, elle s'exécute, journalise son
    // compte rendu et le programme s'arrête (code 0 si elle a réussi, 1 sinon), sans démarrer le serveur.
    // Aucune tâche pour l'instant : la première s'ajoutera ici (docs/tech/tasks.md, « Ajouter une tâche »).

    server::serve(use_cases, config.port).await?;
    Ok(ExitCode::SUCCESS)
}
