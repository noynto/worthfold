//! Configuration, lue une seule fois au démarrage depuis l'environnement (ou un fichier `.env`).

use std::net::{IpAddr, Ipv4Addr};

use anyhow::Context;

pub struct Config {
    /// L'adresse de la base PostgreSQL (elle porte le mot de passe : ne jamais la journaliser).
    pub database_url: String,
    pub port: u16,
    /// L'interface d'écoute : la machine locale par défaut. Seul un conteneur écoute sur toutes les interfaces,
    /// avec un port publié sur `127.0.0.1` uniquement (voir `compose.yaml`).
    pub bind_address: IpAddr,
    /// L'adresse publique (`https://worthfold.example.com`) quand l'application est derrière un proxy HTTPS.
    pub exposure: server::Exposure,
    /// La connexion bancaire (EnableBanking), absente tant qu'elle n'est pas configurée.
    pub bank: Option<BankConfig>,
}

pub struct BankConfig {
    pub application_id: String,
    pub key_path: std::path::PathBuf,
    /// Où la banque renvoie l'utilisateur : doit figurer parmi les adresses déclarées à EnableBanking.
    pub redirect_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let port: u16 = optional("WORTHFOLD_PORT")
            .map(|p| {
                p.parse()
                    .context("WORTHFOLD_PORT doit être un numéro de port.")
            })
            .transpose()?
            .unwrap_or(3000);
        let bind_address: IpAddr = optional("WORTHFOLD_BIND_ADDRESS")
            .map(|a| {
                a.parse().context(
                    "WORTHFOLD_BIND_ADDRESS doit être une adresse IP (127.0.0.1, 0.0.0.0…).",
                )
            })
            .transpose()?
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let public_url = optional("WORTHFOLD_PUBLIC_URL");
        let proxy_hops: usize = optional("WORTHFOLD_PROXY_HOPS")
            .map(|h| {
                h.parse().context(
                    "WORTHFOLD_PROXY_HOPS doit être un nombre : les proxys de confiance devant l'application (0, 1, 2…).",
                )
            })
            .transpose()?
            .unwrap_or(0);
        let exposure = match &public_url {
            Some(url) => server::Exposure::parse(url)?,
            None => server::Exposure::local(),
        }
        .with_proxy_hops(proxy_hops);
        let bank = match (
            optional("WORTHFOLD_ENABLEBANKING_APP_ID"),
            optional("WORTHFOLD_ENABLEBANKING_KEY_PATH"),
        ) {
            (Some(application_id), Some(key_path)) => Some(BankConfig {
                application_id,
                key_path: key_path.into(),
                redirect_url: optional("WORTHFOLD_BANK_REDIRECT_URL").unwrap_or_else(|| {
                    match &public_url {
                        Some(url) => format!("{}/bank/callback", url.trim().trim_end_matches('/')),
                        None => format!("http://localhost:{port}/bank/callback"),
                    }
                }),
            }),
            (None, None) => None,
            _ => anyhow::bail!(
                "WORTHFOLD_ENABLEBANKING_APP_ID et WORTHFOLD_ENABLEBANKING_KEY_PATH vont ensemble : renseigne les deux, ou aucun."
            ),
        };
        Ok(Self {
            bank,
            port,
            bind_address,
            exposure,
            database_url: optional("WORTHFOLD_DATABASE_URL").context(
                "WORTHFOLD_DATABASE_URL est nécessaire : l'adresse de la base PostgreSQL, \
                 par exemple postgres://worthfold:mot_de_passe@localhost:5432/worthfold.",
            )?,
        })
    }
}

/// La variable, si elle est définie et non vide.
pub fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
