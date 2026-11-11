//! Adaptateur web : routes, handlers, templates, mise en forme et graphique.
//! Ne sait pas comment l'application est assemblée : il reçoit les cas d'usage de `bootstrap`.

mod assets;
mod chart;
mod flash;
mod format;
mod guard;
mod handlers;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use application::UseCases;
use axum::Router;
use axum::routing::{get, post};

/// Toutes les routes de l'application, branchées sur les cas d'usage.
pub fn router(use_cases: Arc<UseCases>) -> Router {
    Router::new()
        .route("/", get(handlers::get_dashboard))
        .route("/static/app.css", get(assets::app_css))
        .route(
            "/static/fonts/roboto-flex-latin.woff2",
            get(assets::roboto_flex),
        )
        .route("/static/htmx-2.0.11.min.js", get(assets::htmx))
        .route(
            "/fragments/net-worth",
            get(handlers::get_net_worth_fragment),
        )
        .route("/fragments/assets", get(handlers::get_assets_fragment))
        .route("/fragments/assets/{id}", get(handlers::get_asset_fragment))
        .route("/fragments/chart", get(handlers::get_chart_fragment))
        .route("/net-worth/chart.svg", get(handlers::get_net_worth_chart))
        .route(
            "/assets",
            get(handlers::get_assets_page).post(handlers::post_asset),
        )
        .route(
            "/assets/{id}",
            get(handlers::get_asset_page).post(handlers::post_asset_update),
        )
        .route("/assets/{id}/delete", post(handlers::post_asset_delete))
        .route("/assets/{id}/valuations", post(handlers::post_valuation))
        .route(
            "/assets/{id}/valuations/{valuation_id}/delete",
            post(handlers::post_valuation_delete),
        )
        .fallback(handlers::not_found)
        .with_state(use_cases)
        // Avant toute route, y compris les fichiers statiques et la page introuvable.
        .layer(axum::middleware::from_fn(guard::guard))
}

/// Démarre le serveur web et le fait tourner jusqu'à Ctrl+C ou SIGTERM : il cesse alors d'accepter
/// des connexions, laisse finir les requêtes en cours, puis rend la main.
pub async fn serve(use_cases: Arc<UseCases>, port: u16) -> anyhow::Result<()> {
    // Données sensibles : on n'écoute que sur la machine locale.
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!("worthfold écoute sur http://{address}");
    axum::serve(listener, router(use_cases))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("worthfold s'est arrêté proprement.");
    Ok(())
}

/// Ctrl+C (SIGINT) dans un terminal, SIGTERM envoyé par un orchestrateur (Kubernetes, systemd).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = ?e, "Écoute de Ctrl+C impossible.");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::error!(error = ?e, "Écoute de SIGTERM impossible.");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("Arrêt demandé : fin des requêtes en cours.");
}
