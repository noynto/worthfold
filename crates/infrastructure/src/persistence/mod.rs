mod persisted_assets;
mod persisted_auth;
mod persisted_bank;
mod persisted_budget;
mod persisted_categories;
mod persisted_institutions;
mod persisted_recurrence;
mod persisted_transactions;
mod persisted_valuations;

use std::str::FromStr;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

pub use persisted_assets::PersistedAssets;
pub use persisted_auth::PersistedAuth;
pub use persisted_bank::PersistedBank;
pub use persisted_budget::PersistedBudget;
pub use persisted_categories::PersistedCategories;
pub use persisted_institutions::PersistedInstitutions;
pub use persisted_recurrence::PersistedRecurrence;
pub use persisted_transactions::PersistedTransactions;
pub use persisted_valuations::PersistedValuations;

/// Au plus cette durée pour obtenir une connexion : une base injoignable échoue vite, avec un message.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);
/// Une seule personne se sert de l'application : quelques connexions suffisent largement.
const MAX_CONNECTIONS: u32 = 8;

/// Ouvre la base PostgreSQL et applique les migrations. L'adresse porte le mot de passe : elle n'apparaît
/// jamais dans un message d'erreur.
pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let options = PgConnectOptions::from_str(database_url)
        .context("L'adresse de la base de données est invalide (attendu : postgres://utilisateur:mot_de_passe@hôte:5432/base).")?;
    let pool = PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(options)
        .await
        .context("Impossible de se connecter à la base de données PostgreSQL.")?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Impossible d'appliquer les migrations.")?;
    Ok(pool)
}

/// Une base jetable pour les tests de persistance, créée sur un serveur PostgreSQL de développement.
///
/// L'adresse du serveur vient de `WORTHFOLD_TEST_DATABASE_URL` (une base d'administration où l'on peut
/// créer des bases, par exemple celle du `compose.yaml`) ; sans elle, ces tests sont ignorés.
#[cfg(test)]
pub(crate) mod testdb {
    use super::*;
    use sqlx::postgres::PgConnectOptions;

    pub struct TestDb {
        pub pool: PgPool,
        admin: PgPool,
        name: String,
    }

    impl TestDb {
        /// Supprime la base jetable (ses connexions d'abord).
        pub async fn drop_db(self) {
            self.pool.close().await;
            // Le nom est généré par `database()` (hexadécimal) : aucune saisie n'entre dans cette requête.
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)",
                self.name
            )))
            .execute(&self.admin)
            .await;
        }
    }

    /// Une base neuve et migrée, ou `None` (test ignoré) sans serveur de test configuré.
    pub async fn database() -> Option<TestDb> {
        let Ok(url) = std::env::var("WORTHFOLD_TEST_DATABASE_URL") else {
            eprintln!(
                "Test ignoré : WORTHFOLD_TEST_DATABASE_URL n'est pas définie (voir docs/tech/development.md)."
            );
            return None;
        };
        let admin_options = PgConnectOptions::from_str(&url).expect("adresse de test invalide");
        let admin = PgPool::connect_with(admin_options.clone())
            .await
            .expect("serveur PostgreSQL de test injoignable");
        let name = format!("worthfold_test_{}", shared::random_token());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
            .execute(&admin)
            .await
            .expect("création de la base de test");
        let pool = connect_options(admin_options.database(&name)).await;
        Some(TestDb { pool, admin, name })
    }

    async fn connect_options(options: PgConnectOptions) -> PgPool {
        let pool = PgPool::connect_with(options).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }
}
