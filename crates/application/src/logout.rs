use std::sync::Arc;

use auth::{AuthProvider, hash_token};

use crate::UseCaseError;

pub struct Logout {
    pub auth_provider: Arc<dyn AuthProvider>,
}

pub struct Command {
    pub token: String,
}

impl Logout {
    /// Ferme la session : le jeton cesse de fonctionner, même s'il a été copié.
    pub async fn handle(&self, command: Command) -> Result<(), UseCaseError> {
        self.auth_provider
            .delete_session(&hash_token(&command.token))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auth::Session;
    use testing::InMemoryAuth;

    #[tokio::test]
    async fn ferme_la_session_du_jeton_sans_toucher_aux_autres() {
        let now = chrono::Utc::now();
        let auth = Arc::new(InMemoryAuth::with(
            "une phrase de passe",
            [Session::open("a", now), Session::open("b", now)],
        ));
        Logout {
            auth_provider: auth.clone(),
        }
        .handle(Command { token: "a".into() })
        .await
        .unwrap();
        assert_eq!(auth.session_count(), 1);
        assert!(auth.read_session(&hash_token("b")).await.unwrap().is_some());
    }
}
