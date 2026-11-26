use std::sync::Arc;

use auth::{AuthProvider, hash_token};
use chrono::{DateTime, Utc};

use crate::UseCaseError;

pub struct CheckSession {
    pub auth_provider: Arc<dyn AuthProvider>,
}

pub struct Query {
    /// Le jeton du cookie de session, s'il y en a un.
    pub token: Option<String>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Aucun mot de passe n'existe encore : il faut configurer l'application.
    NotConfigured,
    /// Pas de session valide : il faut se connecter.
    Anonymous,
    Authenticated,
}

impl CheckSession {
    pub async fn handle(&self, query: Query) -> Result<SessionState, UseCaseError> {
        if self.auth_provider.read_credential().await?.is_none() {
            return Ok(SessionState::NotConfigured);
        }
        let Some(token) = query.token.filter(|t| !t.is_empty()) else {
            return Ok(SessionState::Anonymous);
        };
        Ok(
            match self.auth_provider.read_session(&hash_token(&token)).await? {
                Some(session) if session.is_valid_at(query.now) => SessionState::Authenticated,
                _ => SessionState::Anonymous,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auth::Session;
    use chrono::Duration;
    use testing::InMemoryAuth;

    async fn state(
        auth: &Arc<InMemoryAuth>,
        token: Option<&str>,
        now: DateTime<Utc>,
    ) -> SessionState {
        CheckSession {
            auth_provider: auth.clone(),
        }
        .handle(Query {
            token: token.map(str::to_owned),
            now,
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn distingue_non_configure_anonyme_et_connecte() {
        let now = Utc::now();
        assert_eq!(
            state(&Arc::new(InMemoryAuth::default()), Some("x"), now).await,
            SessionState::NotConfigured
        );

        let auth = Arc::new(InMemoryAuth::with(
            "une phrase de passe",
            [Session::open("bon", now)],
        ));
        assert_eq!(
            state(&auth, Some("bon"), now).await,
            SessionState::Authenticated
        );
        assert_eq!(
            state(&auth, Some("faux"), now).await,
            SessionState::Anonymous
        );
        assert_eq!(state(&auth, None, now).await, SessionState::Anonymous);
        assert_eq!(state(&auth, Some(""), now).await, SessionState::Anonymous);
        // Une session expirée ne vaut plus rien.
        assert_eq!(
            state(&auth, Some("bon"), now + Duration::days(15)).await,
            SessionState::Anonymous
        );
    }
}
