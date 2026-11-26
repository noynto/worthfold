//! En-têtes de sécurité de toutes les réponses (voir `docs/tech/security.md`).
//!
//! - **CSP** : tout vient de l'application elle-même ; aucun script ni style en ligne, aucune évaluation de code.
//!   `form-action` est volontairement absent : après l'envoi de « Connecter », le serveur redirige vers la page
//!   de la banque, et la CSP s'applique aussi aux redirections d'un formulaire.
//! - **Cadres** : seule worthfold peut s'afficher dans un cadre (le graphique sans JavaScript est un `<object>` de
//!   la même origine). Une autre page ne peut donc pas l'encadrer pour piéger un clic.
//! - **Cache** : les pages contiennent des données financières ; le navigateur ne les garde pas
//!   (les fichiers statiques gardent leur règle).

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::exposure::Exposure;

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; \
img-src 'self'; connect-src 'self'; object-src 'self'; frame-ancestors 'self'; base-uri 'none'";

pub(crate) async fn secure(
    State(exposure): State<Arc<Exposure>>,
    request: Request,
    next: Next,
) -> Response {
    let is_static = request.uri().path().starts_with("/static/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    let fixed: [(HeaderName, &'static str); 4] = [
        (header::CONTENT_SECURITY_POLICY, CSP),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "SAMEORIGIN"),
        (header::REFERRER_POLICY, "no-referrer"),
    ];
    for (name, value) in fixed {
        headers.insert(name, HeaderValue::from_static(value));
    }
    if exposure.secure() {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
        mark_cookies_secure(headers);
    }
    if !is_static && !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

/// Ajoute `Secure` à chaque cookie posé par la réponse (session, message flash) : en `https`, le navigateur ne
/// les renvoie jamais en clair. Fait ici, une seule fois, plutôt que dans chaque handler qui pose un cookie.
fn mark_cookies_secure(headers: &mut axum::http::HeaderMap) {
    let cookies: Vec<HeaderValue> = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|cookie| match cookie.to_str() {
            Ok(text) if !text.contains("; Secure") => {
                HeaderValue::from_str(&format!("{text}; Secure")).unwrap_or_else(|_| cookie.clone())
            }
            _ => cookie.clone(),
        })
        .collect();
    headers.remove(header::SET_COOKIE);
    for cookie in cookies {
        headers.append(header::SET_COOKIE, cookie);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_cookie_recoit_secure_une_seule_fois() {
        let mut headers = axum::http::HeaderMap::new();
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("a=1; Path=/; HttpOnly"),
        );
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static("b=2; Path=/; Secure"),
        );
        mark_cookies_secure(&mut headers);
        let values: Vec<_> = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(
            values,
            ["a=1; Path=/; HttpOnly; Secure", "b=2; Path=/; Secure"]
        );
    }
}
