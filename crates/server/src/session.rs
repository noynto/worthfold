//! Le cookie de session : le jeton que le navigateur présente à chaque requête.
//!
//! `HttpOnly` (un script de la page ne le lit pas) et `SameSite=Lax`. `Lax` et non `Strict` : le retour de la
//! banque (`/bank/callback`) est une navigation venue d'un autre site, que `Strict` priverait du cookie.
//! La protection contre le CSRF ne repose pas sur ce cookie mais sur la garde (`guard.rs`, `Sec-Fetch-Site`).
//! `Secure` n'est pas écrit ici : `headers.rs` l'ajoute à tout cookie quand l'adresse publique est en `https`
//! (en `http` sur la machine locale, il empêcherait le navigateur de garder le cookie).

use application::SESSION_DAYS;
use axum::http::{HeaderMap, header};

const NAME: &str = "worthfold_session";

pub fn set(token: &str) -> String {
    format!(
        "{NAME}={token}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax",
        SESSION_DAYS * 24 * 60 * 60
    )
}

pub fn clear() -> String {
    format!("{NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
}

/// Le jeton présenté par la requête, s'il y en a un.
pub fn token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|cookie| {
            cookie
                .trim()
                .strip_prefix(&format!("{NAME}="))
                .map(str::to_owned)
        })
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn relit_le_jeton_parmi_les_autres_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("worthfold_flash=m=ok; worthfold_session=abc123; autre=1"),
        );
        assert_eq!(token(&headers).as_deref(), Some("abc123"));
        assert_eq!(token(&HeaderMap::new()), None);
    }

    #[test]
    fn le_cookie_est_ferme_au_script_et_a_la_navigation_croisee_sans_etre_strict() {
        let cookie = set("jeton");
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
        assert!(!cookie.contains("Strict"));
        assert!(clear().contains("Max-Age=0"));
    }
}
