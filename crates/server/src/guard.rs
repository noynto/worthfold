//! Garde de toutes les requêtes, avant les routes : n'accepter que le navigateur de cette machine,
//! sur une page de worthfold.
//!
//! Écouter sur `127.0.0.1` ne suffit pas : une page ouverte sur un autre site, dans le même navigateur,
//! peut envoyer des requêtes au serveur local.
//! - **DNS rebinding** : un domaine piégé (`piege.example`) pointe vers 127.0.0.1 ; pour le navigateur,
//!   ses scripts et worthfold ont alors la même origine, et ils pourraient lire les pages. Le domaine
//!   apparaît pourtant dans l'en-tête `Host` : seuls `127.0.0.1`, `localhost` et l'hôte de l'adresse publique
//!   configurée (`exposure.rs`) sont acceptés.
//! - **CSRF** : un formulaire d'un autre site peut envoyer un `POST` (le CORS n'empêche que la lecture
//!   de la réponse, pas l'envoi). Le navigateur indique la provenance dans `Sec-Fetch-Site`, qu'une page
//!   ne peut pas falsifier : une modification n'est acceptée que depuis worthfold (`same-origin`),
//!   ou sans en-tête (`curl`, un navigateur ancien), qui ne vient donc pas d'une page piégée.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::exposure::Exposure;

pub(crate) async fn guard(
    State(exposure): State<Arc<Exposure>>,
    request: Request,
    next: Next,
) -> Response {
    if !is_known_host(request.headers(), &exposure) {
        tracing::warn!(host = ?request.headers().get(header::HOST), "Requête refusée : hôte inconnu.");
        return refused("Adresse inconnue : ouvre worthfold à son adresse habituelle.");
    }
    if is_modification(request.method()) && !from_worthfold(request.headers()) {
        tracing::warn!(uri = %request.uri(), "Modification refusée : envoyée depuis un autre site.");
        return refused("Modification refusée : elle ne vient pas d'une page de worthfold.");
    }
    next.run(request).await
}

/// `Host` vaut `127.0.0.1` ou `localhost` (avec ou sans port), ou l'hôte public configuré. Sans `Host`, la
/// requête est refusée.
fn is_known_host(headers: &HeaderMap, exposure: &Exposure) -> bool {
    headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|host| exposure.accepts_host(host))
}

fn is_modification(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Une autre application sur `localhost` (autre port) est `same-site` : refusée elle aussi.
fn from_worthfold(headers: &HeaderMap) -> bool {
    match headers.get("sec-fetch-site").and_then(|h| h.to_str().ok()) {
        None => true,
        Some(site) => site == "same-origin" || site == "none",
    }
}

fn refused(message: &'static str) -> Response {
    (StatusCode::FORBIDDEN, message).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, value.parse().unwrap());
        }
        headers
    }

    #[test]
    fn accepte_seulement_les_hotes_connus() {
        let is_local_host = |h: &HeaderMap| is_known_host(h, &Exposure::local());
        for host in ["127.0.0.1:3000", "127.0.0.1", "localhost:3000", "LocalHost"] {
            assert!(is_local_host(&headers(&[("host", host)])), "{host}");
        }
        for host in [
            "piege.example:3000",
            "127.0.0.1.piege.example",
            "localhost.piege.example:80",
        ] {
            assert!(!is_local_host(&headers(&[("host", host)])), "{host}");
        }
        assert!(!is_local_host(&HeaderMap::new()));

        let public = Exposure::parse("https://worthfold.example.com").unwrap();
        assert!(is_known_host(
            &headers(&[("host", "worthfold.example.com")]),
            &public
        ));
        assert!(!is_known_host(
            &headers(&[("host", "piege.example")]),
            &public
        ));
    }

    #[test]
    fn accepte_une_modification_seulement_depuis_worthfold() {
        assert!(from_worthfold(&headers(&[(
            "sec-fetch-site",
            "same-origin"
        )])));
        assert!(from_worthfold(&HeaderMap::new()));
        assert!(!from_worthfold(&headers(&[(
            "sec-fetch-site",
            "cross-site"
        )])));
        assert!(!from_worthfold(&headers(&[(
            "sec-fetch-site",
            "same-site"
        )])));
    }
}
