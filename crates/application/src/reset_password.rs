use std::sync::Arc;

use auth::AuthProvider;

use crate::UseCaseError;

/// Mot de passe oublié : retire le mot de passe et toutes les sessions, l'application redemande une configuration.
/// Réservé à la tâche `WORTHFOLD_RESET_PASSWORD_TASK`, lancée par quelqu'un qui a la main sur la machine ;
/// aucune page web n'y mène.
pub struct ResetPassword {
    pub auth_provider: Arc<dyn AuthProvider>,
}

impl ResetPassword {
    pub async fn handle(&self) -> Result<(), UseCaseError> {
        self.auth_provider.delete_credential().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auth::Session;
    use testing::InMemoryAuth;

    #[tokio::test]
    async fn retire_le_mot_de_passe_et_les_sessions() {
        let auth = Arc::new(InMemoryAuth::with(
            "une phrase de passe",
            [Session::open("a", chrono::Utc::now())],
        ));
        ResetPassword {
            auth_provider: auth.clone(),
        }
        .handle()
        .await
        .unwrap();
        assert!(auth.read_credential().await.unwrap().is_none());
        assert_eq!(auth.session_count(), 0);
    }
}
