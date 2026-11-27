//! Adaptateur web : routes, handlers, templates, mise en forme et graphique.
//! Ne sait pas comment l'application est assemblée : il reçoit les cas d'usage de `bootstrap`.

mod assets;
mod auth;
mod chart;
mod client;
mod exposure;
mod flash;
mod format;
mod guard;
mod handlers;
mod headers;
mod session;

pub use exposure::Exposure;

use std::net::SocketAddr;
use std::sync::Arc;

use application::UseCases;
use axum::Router;
use axum::routing::{get, post};

/// Toutes les routes de l'application, branchées sur les cas d'usage, pour la machine locale seulement.
pub fn router(use_cases: Arc<UseCases>) -> Router {
    router_exposed(use_cases, Exposure::local())
}

/// Comme [`router`], pour une application joignable à l'adresse publique donnée.
pub fn router_exposed(use_cases: Arc<UseCases>, exposure: Exposure) -> Router {
    let exposure = Arc::new(exposure);
    Router::new()
        .route("/", get(handlers::get_dashboard))
        .route(
            "/login",
            get(handlers::get_login).post(handlers::post_login),
        )
        .route(
            "/setup",
            get(handlers::get_setup).post(handlers::post_setup),
        )
        .route("/logout", post(handlers::post_logout))
        .route("/settings", get(handlers::get_settings))
        .route("/account", get(handlers::get_account))
        .route("/account/password", post(handlers::post_change_password))
        .route("/static/app.css", get(assets::app_css))
        .route(
            "/static/fonts/roboto-flex-latin.woff2",
            get(assets::roboto_flex),
        )
        .route("/static/htmx-2.0.11.min.js", get(assets::htmx))
        .route("/static/app.js", get(assets::app_js))
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
        .route("/assets/new", get(handlers::get_asset_new_page))
        .route(
            "/assets/{id}",
            get(handlers::get_asset_page).post(handlers::post_asset_update),
        )
        .route("/assets/{id}/delete", post(handlers::post_asset_delete))
        .route(
            "/assets/{id}/transactions",
            get(handlers::get_transactions_page),
        )
        .route(
            "/assets/{id}/transactions/import",
            post(handlers::post_transactions_import),
        )
        .route(
            "/assets/{id}/transactions/{transaction_id}/category",
            post(handlers::post_transaction_category),
        )
        .route("/payments/new", get(handlers::get_payment_new_page))
        .route(
            "/payments",
            get(handlers::get_payments_page).post(handlers::post_payment),
        )
        .route("/payments/suggestions", post(handlers::post_suggestion))
        .route("/payments/{id}/delete", post(handlers::post_payment_delete))
        .route("/payments/{id}/steps", post(handlers::post_payment_step))
        .route(
            "/payments/{id}/occurrences/{date}",
            post(handlers::post_occurrence),
        )
        .route(
            "/banks",
            get(handlers::get_banks_page).post(handlers::post_institution),
        )
        .route(
            "/banks/{name}/update",
            post(handlers::post_institution_update),
        )
        .route(
            "/banks/{name}/delete",
            post(handlers::post_institution_delete),
        )
        .route("/bank", get(handlers::get_bank_page))
        .route("/bank/connect", post(handlers::post_bank_connect))
        .route("/fragments/banks", get(handlers::get_banks_fragment))
        .route("/bank/callback", get(handlers::get_bank_callback))
        .route("/bank/sync", post(handlers::post_bank_sync))
        .route("/bank/sync-all", post(handlers::post_bank_sync_all))
        .route("/bank/accounts/{uid}/link", post(handlers::post_bank_link))
        .route("/bank/{id}/delete", post(handlers::post_bank_delete))
        .route("/forecast", get(handlers::get_forecast_page))
        .route("/budget", get(handlers::get_budget_current))
        .route("/budget/{month}", get(handlers::get_budget_page))
        .route("/budget/{month}/envelopes", post(handlers::post_envelope))
        .route(
            "/budget/{month}/envelopes/{category_id}/remove",
            post(handlers::post_envelope_remove),
        )
        .route("/budget/{month}/savings", post(handlers::post_savings_plan))
        .route(
            "/categories",
            get(handlers::get_categories_page).post(handlers::post_category),
        )
        .route(
            "/categories/defaults",
            post(handlers::post_default_categories),
        )
        .route("/categories/apply", post(handlers::post_apply_rules))
        .route("/categories/rules", post(handlers::post_rule))
        .route(
            "/categories/rules/{id}/delete",
            post(handlers::post_rule_delete),
        )
        .route(
            "/categories/{id}/delete",
            post(handlers::post_category_delete),
        )
        .route("/assets/{id}/valuations", post(handlers::post_valuation))
        .route(
            "/assets/{id}/valuations/{valuation_id}/delete",
            post(handlers::post_valuation_delete),
        )
        .fallback(handlers::not_found)
        .layer(axum::middleware::from_fn_with_state(
            use_cases.clone(),
            auth::require_session,
        ))
        .with_state(use_cases)
        // Avant toute route, y compris les fichiers statiques et la page introuvable.
        .layer(axum::middleware::from_fn_with_state(
            exposure.clone(),
            client::resolve,
        ))
        .layer(axum::middleware::from_fn_with_state(
            exposure.clone(),
            guard::guard,
        ))
        // Tout en dehors : même une requête refusée par la garde reçoit les en-têtes.
        .layer(axum::middleware::from_fn_with_state(
            exposure,
            headers::secure,
        ))
}

/// Démarre le serveur web et le fait tourner jusqu'à Ctrl+C ou SIGTERM : il cesse alors d'accepter
/// des connexions, laisse finir les requêtes en cours, puis rend la main.
pub async fn serve(
    use_cases: Arc<UseCases>,
    address: SocketAddr,
    exposure: Exposure,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!("worthfold écoute sur http://{address}");
    // `ConnectInfo` : la garde des essais de mot de passe a besoin de l'adresse de la connexion (voir `client.rs`).
    axum::serve(
        listener,
        router_exposed(use_cases, exposure).into_make_service_with_connect_info::<SocketAddr>(),
    )
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
