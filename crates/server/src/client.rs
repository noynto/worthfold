//! Qui fait la requête : l'adresse IP qui sert de clé au frein des essais de mot de passe.
//!
//! - **Sans proxy** (par défaut) : l'adresse de la connexion TCP. `X-Forwarded-For` est ignoré : n'importe quel
//!   client peut l'écrire, et s'y fier permettrait de changer d'« adresse » à chaque essai.
//! - **Derrière `n` proxys de confiance** (`WORTHFOLD_PROXY_HOPS`) : chaque proxy ajoute à droite de
//!   `X-Forwarded-For` l'adresse de celui qui lui parle. L'adresse du client est donc la `n`-ième en partant de la
//!   droite ; ce qui est à gauche vient du client et ne vaut rien. S'il y a moins de `n` adresses, la requête n'est
//!   pas passée par les proxys attendus : on retombe sur la connexion TCP.
//!
//! Une adresse IPv6 est réduite à son préfixe /64 (celui d'un abonné) : sinon, un client en change à volonté.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

use crate::exposure::Exposure;

/// La clé du client, posée dans les extensions de la requête par [`resolve`].
#[derive(Clone, Debug)]
pub(crate) struct Client(pub String);

/// Sans adresse connue (les tests qui appellent le routeur sans passer par un socket).
const UNKNOWN: &str = "inconnu";

pub(crate) async fn resolve(
    State(exposure): State<Arc<Exposure>>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    let key = address(request.headers(), peer, exposure.proxy_hops())
        .map(key)
        .unwrap_or_else(|| UNKNOWN.to_owned());
    request.extensions_mut().insert(Client(key));
    next.run(request).await
}

fn address(headers: &HeaderMap, peer: Option<IpAddr>, hops: usize) -> Option<IpAddr> {
    if hops == 0 {
        return peer;
    }
    let forwarded: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .collect();
    forwarded
        .len()
        .checked_sub(hops)
        .and_then(|index| forwarded[index].parse().ok())
        .or(peer)
}

fn key(address: IpAddr) -> String {
    match address {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forwarded(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", value.parse().unwrap());
        }
        headers
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn sans_proxy_l_en_tete_est_ignore() {
        let headers = forwarded(&["1.2.3.4"]);
        assert_eq!(
            address(&headers, Some(ip("10.0.0.5")), 0),
            Some(ip("10.0.0.5"))
        );
        assert_eq!(address(&headers, None, 0), None);
    }

    #[test]
    fn avec_un_proxy_c_est_la_derniere_adresse_et_pas_celle_que_le_client_ecrit() {
        // Le client a écrit 6.6.6.6 pour se faire passer pour un autre ; l'Ingress a ajouté la vraie.
        let headers = forwarded(&["6.6.6.6, 203.0.113.7"]);
        assert_eq!(
            address(&headers, Some(ip("10.0.0.5")), 1),
            Some(ip("203.0.113.7"))
        );
    }

    #[test]
    fn avec_deux_proxys_c_est_l_avant_derniere_adresse_meme_sur_plusieurs_en_tetes() {
        let headers = forwarded(&["6.6.6.6", "203.0.113.7, 10.1.1.1"]);
        assert_eq!(
            address(&headers, Some(ip("10.0.0.5")), 2),
            Some(ip("203.0.113.7"))
        );
    }

    #[test]
    fn sans_assez_d_adresses_ou_avec_du_texte_on_retombe_sur_la_connexion() {
        let peer = Some(ip("10.0.0.5"));
        assert_eq!(address(&HeaderMap::new(), peer, 1), peer);
        assert_eq!(address(&forwarded(&["203.0.113.7"]), peer, 2), peer);
        assert_eq!(address(&forwarded(&["pas une adresse"]), peer, 1), peer);
    }

    #[test]
    fn une_adresse_ipv6_est_reduite_a_son_prefixe() {
        assert_eq!(
            key(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd")),
            key(ip("2001:db8:1:2::1"))
        );
        assert_ne!(key(ip("2001:db8:1:2::1")), key(ip("2001:db8:1:3::1")));
        assert_eq!(key(ip("203.0.113.7")), "203.0.113.7");
    }
}
