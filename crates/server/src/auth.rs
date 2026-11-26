//! Garde d'authentification, après la garde des requêtes : sans session valide, on ne voit que
//! la connexion (ou la première configuration, tant qu'aucun mot de passe n'existe) et les fichiers statiques.
//!
//! Un seul cas d'usage répond (`CheckSession`) : le mot de passe est-il configuré, et ce jeton est-il valide ?

use std::sync::Arc;

use application::UseCases;
use application::check_session::{self, SessionState};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};

use crate::handlers::now;
use crate::session;

pub(crate) async fn require_session(
    State(app): State<Arc<UseCases>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    // Les fichiers statiques (feuille de style, htmx, police) ne contiennent aucune donnée : les pages de
    // connexion en ont besoin avant toute session.
    if path.starts_with("/static/") {
        return next.run(request).await;
    }
    let on_login = path == "/login";
    let on_setup = path == "/setup";
    let state = match app
        .check_session
        .handle(check_session::Query {
            token: session::token(request.headers()),
            now: now(),
        })
        .await
    {
        Ok(state) => state,
        Err(e) => {
            tracing::error!(error = ?e, "Vérification de la session impossible.");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Service indisponible.").into_response();
        }
    };
    match state {
        SessionState::Authenticated if on_login || on_setup => Redirect::to("/").into_response(),
        SessionState::Authenticated => next.run(request).await,
        SessionState::NotConfigured if on_setup => next.run(request).await,
        SessionState::NotConfigured => redirect(request.headers(), "/setup"),
        SessionState::Anonymous if on_login => next.run(request).await,
        SessionState::Anonymous => redirect(request.headers(), "/login"),
    }
}

/// Vers la page d'entrée. Une requête htmx (fragment, formulaire) ne peut pas suivre une redirection
/// de page : elle reçoit `HX-Redirect`, qui fait changer la page entière.
fn redirect(headers: &HeaderMap, to: &'static str) -> Response {
    if headers.contains_key("HX-Request") {
        (StatusCode::UNAUTHORIZED, [("HX-Redirect", to)], "").into_response()
    } else {
        Redirect::to(to).into_response()
    }
}
