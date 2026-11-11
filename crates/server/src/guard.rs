//! Garde de toutes les requêtes, avant les routes : n'accepter que le navigateur de cette machine,
//! sur une page de worthfold.
//!
//! Écouter sur `127.0.0.1` ne suffit pas : une page ouverte sur un autre site, dans le même navigateur,
//! peut envoyer des requêtes au serveur local.
//! - **DNS rebinding** : un domaine piégé (`piege.example`) pointe vers 127.0.0.1 ; pour le navigateur,
//!   ses scripts et worthfold ont alors la même origine, et ils pourraient lire les pages. Le domaine
//!   apparaît pourtant dans l'en-tête `Host` : seuls `127.0.0.1` et `localhost` sont acceptés.
//! - **CSRF** : un formulaire d'un autre site peut envoyer un `POST` (le CORS n'empêche que la lecture
//!   de la réponse, pas l'envoi). Le navigateur indique la provenance dans `Sec-Fetch-Site`, qu'une page
//!   ne peut pas falsifier : une modification n'est acceptée que depuis worthfold (`same-origin`),
//!   ou sans en-tête (`curl`, un navigateur ancien), qui ne vient donc pas d'une page piégée.

use axum::extract::Request;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub(crate) async fn guard(request: Request, next: Next) -> Response {
    if !is_local_host(request.headers()) {
        tracing::warn!(host = ?request.headers().get(header::HOST), "Requête refusée : hôte inconnu.");
        return refused("Adresse inconnue : ouvre worthfold sur http://127.0.0.1.");
    }
    if is_modification(request.method()) && !from_worthfold(request.headers()) {
        tracing::warn!(uri = %request.uri(), "Modification refusée : envoyée depuis un autre site.");
        return refused("Modification refusée : elle ne vient pas d'une page de worthfold.");
    }
    next.run(request).await
}

/// `Host` vaut `127.0.0.1` ou `localhost`, avec ou sans port. Sans `Host`, la requête est refusée.
fn is_local_host(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let name = match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    };
    name == "127.0.0.1" || name.eq_ignore_ascii_case("localhost")
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
    fn accepte_seulement_les_hotes_locaux() {
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
