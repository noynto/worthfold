//! Les réponses : rendu des templates, réponse d'un formulaire (htmx ou non), pannes.

use application::UseCaseError;
use askama::Template;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};

use super::WORTH_CHANGED;
use crate::flash;

// ── Réponses ─────────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "partials/inline_error.html")]
struct InlineError<'a> {
    message: &'a str,
}

#[derive(Template)]
#[template(path = "partials/snackbar.html")]
struct Snackbar<'a> {
    message: &'a str,
}

/// La réponse d'un formulaire, selon qu'il a été envoyé par htmx ou sans JavaScript.
pub(crate) struct Reply {
    htmx: bool,
    /// La page du formulaire, pour y revenir sans JavaScript.
    back_to: String,
}

impl Reply {
    pub(crate) fn new(headers: &HeaderMap, back_to: &str) -> Self {
        Self {
            htmx: headers.contains_key("HX-Request"),
            back_to: back_to.to_owned(),
        }
    }

    /// Le patrimoine a changé : htmx vide le message d'erreur (corps vide dans la cible), prévient les
    /// fragments (`HX-Trigger`) et affiche la confirmation dans la snackbar (échange hors cible).
    pub(crate) fn worth_changed(&self, message: &str) -> Response {
        if self.htmx {
            match (Snackbar { message }).render() {
                // Sans espaces autour : une fois la snackbar extraite, la cible (zone d'erreur) doit
                // recevoir un contenu vraiment vide, sinon `empty:hidden` ne la masque plus.
                Ok(html) => (
                    [("HX-Trigger", WORTH_CHANGED)],
                    Html(html.trim().to_owned()),
                )
                    .into_response(),
                Err(e) => {
                    tracing::error!(error = ?e, "Rendu de la confirmation impossible.");
                    ([("HX-Trigger", WORTH_CHANGED)], "").into_response()
                }
            }
        } else {
            // Sans JavaScript : la confirmation suit la redirection (message flash).
            (
                [(header::SET_COOKIE, flash::set(message))],
                Redirect::to(&self.back_to),
            )
                .into_response()
        }
    }

    /// Aller sur une autre page, avec une confirmation affichée dans sa snackbar (message flash).
    pub(crate) fn redirect(&self, to: &str, message: &str) -> Response {
        let cookie = [(header::SET_COOKIE, flash::set(message))];
        if self.htmx {
            (cookie, [("HX-Redirect", to)], "").into_response()
        } else {
            (cookie, Redirect::to(to)).into_response()
        }
    }

    /// Un refus : sous le formulaire (422) avec htmx, en haut de la page sinon.
    pub(crate) fn refusal(&self, message: &str) -> Response {
        if self.htmx {
            match (InlineError { message }).render() {
                Ok(html) => (StatusCode::UNPROCESSABLE_ENTITY, Html(html)).into_response(),
                Err(e) => {
                    tracing::error!(error = ?e, "Rendu du message d'erreur impossible.");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        } else {
            let query = serde_urlencoded::to_string([("error", message)]).unwrap_or_default();
            Redirect::to(&format!("{}?{query}", self.back_to)).into_response()
        }
    }

    pub(crate) fn error(&self, error: UseCaseError) -> Response {
        match error {
            UseCaseError::Refusal(message) => self.refusal(&message),
            UseCaseError::Technical(e) => {
                tracing::error!(error = ?e, "Erreur technique.");
                let message = "Une erreur technique est survenue.";
                if self.htmx {
                    (StatusCode::INTERNAL_SERVER_ERROR, message).into_response()
                } else {
                    technical_message()
                }
            }
        }
    }
}

pub(crate) fn render(page: impl Template) -> Response {
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "Rendu du template impossible.");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Une page qui n'a qu'un message : introuvable ou indisponible, avec un lien pour repartir.
#[derive(Template)]
#[template(path = "message_page.html")]
struct MessagePage<'a> {
    section: &'static str,
    title: &'a str,
    message: &'a str,
    link: &'static str,
    link_label: &'static str,
}

fn message_page(status: StatusCode, page: MessagePage) -> Response {
    let mut response = render(page);
    if response.status().is_success() {
        *response.status_mut() = status;
    }
    response
}

/// 404 : la page demandée n'existe pas, ou plus (`message` dit pourquoi).
pub(crate) fn not_found(title: &str, message: &str) -> Response {
    message_page(
        StatusCode::NOT_FOUND,
        MessagePage {
            section: "",
            title,
            message,
            link: "/assets",
            link_label: "Voir tous les actifs",
        },
    )
}

/// Une page ne redirige jamais vers elle-même en cas d'erreur (boucle) : 500 journalisé.
pub(crate) fn technical_page(error: UseCaseError) -> Response {
    tracing::error!(error = ?error, "Page indisponible.");
    technical_message()
}

/// Un fragment en panne garde sa place : un court message, jamais une page d'erreur.
pub(crate) fn technical_fragment(error: UseCaseError) -> Response {
    tracing::error!(error = ?error, "Fragment indisponible.");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Information indisponible pour le moment.",
    )
        .into_response()
}

/// 500 sans JavaScript : la panne est journalisée, l'utilisateur sait qu'il peut réessayer.
fn technical_message() -> Response {
    message_page(
        StatusCode::INTERNAL_SERVER_ERROR,
        MessagePage {
            section: "",
            title: "Page indisponible",
            message: "Une erreur technique est survenue. Réessaie dans un instant ; tes données ne sont pas perdues.",
            link: "/",
            link_label: "Revenir au tableau de bord",
        },
    )
}
