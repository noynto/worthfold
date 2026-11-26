//! Connexion, première configuration, déconnexion et changement de mot de passe.
//! Ces pages n'ont pas de navigation : on y est avant d'entrer, ou pour en sortir.

use application::{
    MIN_PASSWORD_LENGTH, SESSION_DAYS, change_password, login, logout, setup_password,
};
use askama::Template;
use axum::Form;
use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::AppState;
use super::reply::{Reply, render, technical_page};
use super::views::{ErrorParams, now, with_flash_consumed};
use crate::client::Client;
use crate::{flash, session};

#[derive(Template)]
#[template(path = "login_page.html")]
struct LoginPage {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "setup_page.html")]
struct SetupPage {
    error: Option<String>,
    min_length: usize,
}

#[derive(Template)]
#[template(path = "account_page.html")]
struct AccountPage {
    section: &'static str,
    error: Option<String>,
    flash: Option<String>,
    min_length: usize,
    session_days: i64,
}

/// Ouvre une session : le cookie, puis la page d'accueil.
fn signed_in(token: &str, to: &str) -> Response {
    (
        [(header::SET_COOKIE, session::set(token))],
        Redirect::to(to),
    )
        .into_response()
}

// ── GET /login, POST /login ──────────────────────────────────────────────────

pub async fn get_login(Query(params): Query<ErrorParams>) -> Response {
    render(LoginPage {
        error: params.error,
    })
}

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

pub async fn post_login(
    State(app): AppState,
    headers: HeaderMap,
    Extension(Client(client)): Extension<Client>,
    Form(form): Form<LoginForm>,
) -> Response {
    let reply = Reply::new(&headers, "/login");
    match app
        .login
        .handle(login::Command {
            client,
            password: form.password,
            now: now(),
        })
        .await
    {
        Ok(output) => signed_in(&output.token, "/"),
        Err(e) => reply.error(e),
    }
}

// ── GET /setup, POST /setup ──────────────────────────────────────────────────

pub async fn get_setup(Query(params): Query<ErrorParams>) -> Response {
    render(SetupPage {
        error: params.error,
        min_length: MIN_PASSWORD_LENGTH,
    })
}

#[derive(Deserialize)]
pub struct SetupForm {
    setup_code: String,
    password: String,
    confirmation: String,
}

pub async fn post_setup(
    State(app): AppState,
    headers: HeaderMap,
    Extension(Client(client)): Extension<Client>,
    Form(form): Form<SetupForm>,
) -> Response {
    let reply = Reply::new(&headers, "/setup");
    match app
        .setup_password
        .handle(setup_password::Command {
            client,
            setup_code: form.setup_code,
            password: form.password,
            confirmation: form.confirmation,
            now: now(),
        })
        .await
    {
        Ok(output) => signed_in(&output.token, "/"),
        Err(e) => reply.error(e),
    }
}

// ── POST /logout ─────────────────────────────────────────────────────────────

pub async fn post_logout(State(app): AppState, headers: HeaderMap) -> Response {
    if let Some(token) = session::token(&headers)
        && let Err(e) = app.logout.handle(logout::Command { token }).await
    {
        return technical_page(e);
    }
    (
        [(header::SET_COOKIE, session::clear())],
        Redirect::to("/login"),
    )
        .into_response()
}

// ── GET /account, POST /account/password ─────────────────────────────────────

pub async fn get_account(Query(params): Query<ErrorParams>, headers: HeaderMap) -> Response {
    let flash = flash::take(&headers);
    let had_flash = flash.is_some();
    let page = render(AccountPage {
        section: "account",
        error: params.error,
        flash,
        min_length: MIN_PASSWORD_LENGTH,
        session_days: SESSION_DAYS,
    });
    with_flash_consumed(page, had_flash)
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    current: String,
    password: String,
    confirmation: String,
}

pub async fn post_change_password(
    State(app): AppState,
    headers: HeaderMap,
    Extension(Client(client)): Extension<Client>,
    Form(form): Form<ChangePasswordForm>,
) -> Response {
    let reply = Reply::new(&headers, "/account");
    match app
        .change_password
        .handle(change_password::Command {
            client,
            current: form.current,
            password: form.password,
            confirmation: form.confirmation,
            now: now(),
        })
        .await
    {
        // Toutes les sessions ont été fermées : la nouvelle remplace le cookie.
        // `AppendHeaders` : deux `Set-Cookie`, et non le second qui écraserait le premier.
        Ok(output) => (
            AppendHeaders([
                (header::SET_COOKIE, session::set(&output.token)),
                (header::SET_COOKIE, flash::set("Mot de passe changé.")),
            ]),
            Redirect::to("/account"),
        )
            .into_response(),
        Err(e) => reply.error(e),
    }
}
