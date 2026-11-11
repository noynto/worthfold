//! Configuration, lue une seule fois au démarrage depuis l'environnement (ou un fichier `.env`).

use anyhow::Context;

pub struct Config {
    pub database_url: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: optional("WORTHFOLD_DATABASE_URL")
                .unwrap_or_else(|| "sqlite://worthfold.db".into()),
            port: optional("WORTHFOLD_PORT")
                .map(|p| {
                    p.parse()
                        .context("WORTHFOLD_PORT doit être un numéro de port.")
                })
                .transpose()?
                .unwrap_or(3000),
        })
    }
}

/// La variable, si elle est définie et non vide.
pub fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
