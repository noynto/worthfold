//! Point d'entrée de worthfold (binaire `worthfold`) et composition root.
//!
//! Lit la configuration, assemble les adaptateurs et les cas d'usage, puis lance **soit une tâche**,
//! **soit le serveur web** : comme le `Bootstrap` d'EOSA, un seul binaire (et une seule image) sert aux
//! deux, et une variable d'environnement choisit la tâche (voir docs/tech/tasks.md).

mod assembly;
mod config;
mod healthcheck;
mod tasks;

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use config::Config;

fn main() -> ExitCode {
    // `worthfold healthcheck` : la sonde du conteneur (l'image n'a ni shell ni curl). Sans runtime ni base.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        return healthcheck::run();
    }
    run_async()
}

#[tokio::main]
async fn run_async() -> ExitCode {
    let _ = dotenvy::dotenv();
    // Niveau `info` par défaut ; `RUST_LOG` (par exemple `RUST_LOG=warn`) le change.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy(),
        )
        .init();
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
    let assembly::Assembled {
        use_cases,
        setup_code,
    } = assembly::build(&config).await?;
    let use_cases = Arc::new(use_cases);

    // Tâches one-shot : si la variable d'activation d'une tâche est définie, elle s'exécute, journalise son
    // compte rendu et le programme s'arrête (code 0 si elle a réussi, 1 sinon), sans démarrer le serveur.
    if config::optional("WORTHFOLD_BANK_SYNC_TASK").is_some() {
        return Ok(tasks::bank_sync(&use_cases).await);
    }

    if config::optional("WORTHFOLD_RESET_PASSWORD_TASK").is_some() {
        return Ok(tasks::reset_password(&use_cases).await);
    }

    tasks::announce_setup(&use_cases, &setup_code).await;
    server::serve(
        use_cases,
        SocketAddr::new(config.bind_address, config.port),
        config.exposure,
    )
    .await?;
    Ok(ExitCode::SUCCESS)
}
