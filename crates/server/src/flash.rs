//! Message « flash » : la confirmation d'une action qui redirige vers une autre page.
//!
//! La réponse qui redirige pose un cookie de courte durée avec le message ; la page d'arrivée le lit,
//! l'affiche dans sa snackbar et l'efface aussitôt. Plutôt qu'un paramètre d'adresse (`?ok=…`), le
//! message ne reste pas dans l'URL et ne se réaffiche pas quand on recharge la page.

use axum::http::{HeaderMap, header};

const NAME: &str = "worthfold_flash";

/// L'en-tête `Set-Cookie` qui transporte le message (encodé), valable une minute.
pub fn set(message: &str) -> String {
    let value = serde_urlencoded::to_string([("m", message)]).unwrap_or_default();
    format!("{NAME}={value}; Path=/; Max-Age=60; HttpOnly; SameSite=Strict")
}

/// L'en-tête `Set-Cookie` qui efface le message.
pub fn clear() -> String {
    format!("{NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict")
}

/// Le message transporté par la requête, s'il y en a un.
pub fn take(headers: &HeaderMap) -> Option<String> {
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
        .and_then(|value| serde_urlencoded::from_str::<Vec<(String, String)>>(&value).ok())
        .and_then(|pairs| {
            pairs
                .into_iter()
                .find(|(key, _)| key == "m")
                .map(|(_, message)| message)
        })
        .filter(|message| !message.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn request_with(cookie_header: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(cookie_header).unwrap(),
        );
        headers
    }

    #[test]
    fn relit_le_message_pose_par_la_redirection() {
        let message = "« Maison » supprimé, avec son historique ; 3 = trois & plus.";
        let cookie = set(message);
        let pair = cookie.split(';').next().unwrap();
        assert_eq!(
            take(&request_with(&format!("autre=1; {pair}; encore=2"))),
            Some(message.to_owned())
        );
    }

    #[test]
    fn la_valeur_du_cookie_ne_contient_aucun_separateur() {
        let pair = set("a; b, c \"d\" \\ e")
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let value = pair.strip_prefix("worthfold_flash=").unwrap();
        assert!(!value.contains([';', ',', ' ', '"', '\\']), "{value}");
    }

    #[test]
    fn sans_cookie_pas_de_message() {
        assert_eq!(take(&HeaderMap::new()), None);
        assert_eq!(take(&request_with("worthfold_flash=")), None);
    }
}
