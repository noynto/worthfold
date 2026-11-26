use anyhow::Context;
use async_trait::async_trait;
use auth::{AuthProvider, Credential, Session};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

pub struct PersistedAuth {
    pool: PgPool,
}

impl PersistedAuth {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct SessionRow {
    token_hash: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        Session {
            token_hash: row.token_hash,
            created_at: row.created_at,
            expires_at: row.expires_at,
        }
    }
}

#[async_trait]
impl AuthProvider for PersistedAuth {
    async fn read_credential(&self) -> anyhow::Result<Option<Credential>> {
        let hash: Option<(String,)> =
            sqlx::query_as("SELECT password_hash FROM credentials WHERE id = 1")
                .fetch_optional(&self.pool)
                .await
                .context("Impossible de lire le mot de passe.")?;
        Ok(hash.map(|(password_hash,)| Credential { password_hash }))
    }

    async fn write_credential(&self, credential: &Credential) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO credentials (id, password_hash) VALUES (1, $1) \
             ON CONFLICT (id) DO UPDATE SET password_hash = excluded.password_hash",
        )
        .bind(&credential.password_hash)
        .execute(&self.pool)
        .await
        .context("Impossible d'enregistrer le mot de passe.")?;
        Ok(())
    }

    async fn write_session(&self, session: &Session) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO sessions (token_hash, created_at, expires_at) VALUES ($1, $2, $3)",
        )
        .bind(&session.token_hash)
        .bind(session.created_at)
        .bind(session.expires_at)
        .execute(&self.pool)
        .await
        .context("Impossible d'ouvrir la session.")?;
        Ok(())
    }

    async fn read_session(&self, token_hash: &str) -> anyhow::Result<Option<Session>> {
        Ok(sqlx::query_as::<_, SessionRow>(
            "SELECT token_hash, created_at, expires_at FROM sessions WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .context("Impossible de lire la session.")?
        .map(Session::from))
    }

    async fn delete_session(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .context("Impossible de fermer la session.")?;
        Ok(())
    }

    async fn delete_all_sessions(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions")
            .execute(&self.pool)
            .await
            .context("Impossible de fermer les sessions.")?;
        Ok(())
    }

    async fn delete_expired_sessions(&self, now: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await
            .context("Impossible de retirer les sessions expirées.")?;
        Ok(())
    }

    async fn delete_credential(&self) -> anyhow::Result<()> {
        let mut db_transaction = self
            .pool
            .begin()
            .await
            .context("Impossible d'ouvrir la transaction de base de données.")?;
        sqlx::query("DELETE FROM sessions")
            .execute(&mut *db_transaction)
            .await
            .context("Impossible de fermer les sessions.")?;
        sqlx::query("DELETE FROM credentials")
            .execute(&mut *db_transaction)
            .await
            .context("Impossible de retirer le mot de passe.")?;
        db_transaction
            .commit()
            .await
            .context("Impossible de valider la réinitialisation.")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::testdb::database;
    use chrono::{Duration, TimeZone};

    #[tokio::test]
    async fn le_mot_de_passe_et_les_sessions_font_l_aller_retour() {
        let Some(db) = database().await else { return };
        let auth = PersistedAuth::new(db.pool.clone());
        assert_eq!(auth.read_credential().await.unwrap(), None);

        auth.write_credential(&Credential {
            password_hash: "h1".into(),
        })
        .await
        .unwrap();
        auth.write_credential(&Credential {
            password_hash: "h2".into(),
        })
        .await
        .unwrap();
        assert_eq!(
            auth.read_credential().await.unwrap().unwrap().password_hash,
            "h2"
        );

        let now = Utc.with_ymd_and_hms(2026, 11, 26, 12, 0, 0).unwrap();
        let live = Session::open("vivant", now);
        let old = Session::open("ancien", now - Duration::days(30));
        auth.write_session(&live).await.unwrap();
        auth.write_session(&old).await.unwrap();
        assert_eq!(
            auth.read_session(&live.token_hash).await.unwrap(),
            Some(live.clone())
        );

        // Les sessions expirées partent ; les autres restent.
        auth.delete_expired_sessions(now).await.unwrap();
        assert_eq!(auth.read_session(&old.token_hash).await.unwrap(), None);
        assert!(auth.read_session(&live.token_hash).await.unwrap().is_some());

        auth.delete_session(&live.token_hash).await.unwrap();
        assert_eq!(auth.read_session(&live.token_hash).await.unwrap(), None);

        // Réinitialiser retire le mot de passe et toute session.
        auth.write_session(&live).await.unwrap();
        auth.delete_credential().await.unwrap();
        assert_eq!(auth.read_credential().await.unwrap(), None);
        assert_eq!(auth.read_session(&live.token_hash).await.unwrap(), None);
        db.drop_db().await;
    }
}
