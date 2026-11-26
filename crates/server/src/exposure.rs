//! Où worthfold est joignable : sur la machine locale seulement (par défaut), ou à une adresse publique.
//!
//! Derrière un proxy ou un Ingress qui termine HTTPS, l'application reste en `http` dans le cluster mais le
//! navigateur la voit en `https` sous un nom de domaine. Cette adresse publique (`WORTHFOLD_PUBLIC_URL`) donne :
//! - l'hôte que la garde accepte en plus de `127.0.0.1` et `localhost` (contre le DNS rebinding) ;
//! - l'attribut `Secure` des cookies et l'en-tête HSTS, quand l'adresse est en `https`.

use anyhow::{Context, bail};

/// L'adresse publique de l'application, validée.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exposure {
    /// Hôte (et port s'il n'est pas celui du schéma), en minuscules, tel que le navigateur l'envoie dans `Host`.
    public_host: Option<String>,
    secure: bool,
    /// Combien de proxys de confiance sont devant l'application (0 : aucun, la connexion TCP est le client).
    proxy_hops: usize,
}

impl Exposure {
    /// La machine locale seulement : ni hôte public, ni cookie `Secure`.
    pub fn local() -> Self {
        Self::default()
    }

    /// Lit une adresse comme `https://worthfold.example.com` (sans chemin). Refuse tout autre schéma que
    /// `http` ou `https`, un chemin, une requête et des identifiants dans l'adresse.
    pub fn parse(url: &str) -> anyhow::Result<Self> {
        let url = url.trim();
        let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
            (true, rest)
        } else if let Some(rest) = url.strip_prefix("http://") {
            (false, rest)
        } else {
            bail!(
                "WORTHFOLD_PUBLIC_URL doit commencer par https:// (ou http://), par exemple https://worthfold.example.com."
            );
        };
        let authority = rest.strip_suffix('/').unwrap_or(rest);
        if authority.is_empty()
            || authority
                .chars()
                .any(|c| matches!(c, '/' | '?' | '#' | '@') || c.is_whitespace())
        {
            bail!(
                "WORTHFOLD_PUBLIC_URL doit être une origine seule (schéma, nom de domaine, port éventuel), sans chemin ni identifiants."
            );
        }
        let host = authority.to_ascii_lowercase();
        let (name, port) = match host.rsplit_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host.as_str(), None),
        };
        if name.is_empty() || name.contains(':') {
            bail!(
                "WORTHFOLD_PUBLIC_URL : nom de domaine invalide (les adresses IPv6 ne sont pas prises en charge)."
            );
        }
        if let Some(port) = port {
            port.parse::<u16>()
                .context("WORTHFOLD_PUBLIC_URL : le port doit être un numéro.")?;
        }
        // Le navigateur n'envoie pas le port par défaut du schéma dans `Host`.
        let default_port = if secure { ":443" } else { ":80" };
        let public_host = host
            .strip_suffix(default_port)
            .map(str::to_owned)
            .unwrap_or(host);
        Ok(Self {
            public_host: Some(public_host),
            secure,
            proxy_hops: 0,
        })
    }

    /// Déclare `hops` proxys de confiance devant l'application (un Ingress : 1 ; un répartiteur de charge devant
    /// l'Ingress : 2). Ils ajoutent chacun l'adresse de leur client à `X-Forwarded-For` ; voir `client.rs`.
    pub fn with_proxy_hops(mut self, hops: usize) -> Self {
        self.proxy_hops = hops;
        self
    }

    pub(crate) fn proxy_hops(&self) -> usize {
        self.proxy_hops
    }

    /// Les cookies portent `Secure` et les réponses `Strict-Transport-Security`.
    pub fn secure(&self) -> bool {
        self.secure
    }

    /// `Host` (avec ou sans port) accepté : la machine locale, ou l'hôte public.
    pub(crate) fn accepts_host(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        let name = match host.rsplit_once(':') {
            Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
            _ => host.as_str(),
        };
        name == "127.0.0.1"
            || name == "localhost"
            || self.public_host.as_deref() == Some(host.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_adresse_https_donne_l_hote_et_le_cookie_secure() {
        let exposure = Exposure::parse("https://Worthfold.Example.com/").unwrap();
        assert!(exposure.secure());
        assert!(exposure.accepts_host("worthfold.example.com"));
        assert!(exposure.accepts_host("WORTHFOLD.example.com"));
        assert!(!exposure.accepts_host("piege.example.com"));
        assert!(!exposure.accepts_host("worthfold.example.com.piege.example"));
    }

    #[test]
    fn le_port_par_defaut_du_schema_est_omis() {
        let exposure = Exposure::parse("https://worthfold.example.com:443").unwrap();
        assert!(exposure.accepts_host("worthfold.example.com"));
        let exposure = Exposure::parse("http://nas.local:8080").unwrap();
        assert!(!exposure.secure());
        assert!(exposure.accepts_host("nas.local:8080"));
        assert!(!exposure.accepts_host("nas.local"));
    }

    #[test]
    fn la_machine_locale_reste_acceptee() {
        for exposure in [
            Exposure::local(),
            Exposure::parse("https://worthfold.example.com").unwrap(),
        ] {
            assert!(exposure.accepts_host("127.0.0.1:3000"));
            assert!(exposure.accepts_host("localhost"));
        }
        assert!(!Exposure::local().accepts_host("worthfold.example.com"));
    }

    #[test]
    fn refuse_les_adresses_qui_ne_sont_pas_une_origine() {
        for url in [
            "worthfold.example.com",
            "ftp://worthfold.example.com",
            "https://",
            "https://worthfold.example.com/app",
            "https://worthfold.example.com?x=1",
            "https://user@worthfold.example.com",
            "https://worthfold.example.com:port",
            "https://[::1]:3000",
        ] {
            assert!(Exposure::parse(url).is_err(), "{url}");
        }
    }
}
