use std::sync::Arc;

use auth::{AuthProvider, PasswordHasher, Session, Throttle};
use chrono::{DateTime, Utc};

use crate::UseCaseError;
use crate::session::{new_token, too_many_attempts};

pub struct Login {
    pub auth_provider: Arc<dyn AuthProvider>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub throttle: Arc<Throttle>,
}

pub struct Command {
    pub password: String,
    /// Qui essaie (son adresse IP) : le frein des essais est propre à chaque client.
    pub client: String,
    pub now: DateTime<Utc>,
}

pub struct Output {
    pub token: String,
}

impl Login {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let Some(credential) = self.auth_provider.read_credential().await? else {
            return Err(UseCaseError::refusal(
                "worthfold n'est pas encore configuré : choisis d'abord un mot de passe.",
            ));
        };
        if let Err(seconds) = self.throttle.check(&command.client, command.now) {
            return Err(UseCaseError::refusal(too_many_attempts(seconds)));
        }
        if !self
            .hasher
            .verify(&command.password, &credential.password_hash)?
        {
            self.throttle.record_failure(&command.client, command.now);
            return Err(UseCaseError::refusal("Mot de passe incorrect."));
        }
        self.throttle.record_success(&command.client);
        let token = new_token();
        self.auth_provider
            .write_session(&Session::open(&token, command.now))
            .await?;
        // Au passage : les sessions expirées n'ont plus de raison d'être gardées.
        self.auth_provider
            .delete_expired_sessions(command.now)
            .await?;
        Ok(Output { token })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{InMemoryAuth, PlainHasher};

    fn use_case(auth: Arc<InMemoryAuth>) -> Login {
        Login {
            auth_provider: auth,
            hasher: Arc::new(PlainHasher),
            throttle: Arc::default(),
        }
    }

    fn command(password: &str) -> Command {
        Command {
            password: password.into(),
            client: "203.0.113.7".into(),
            now: Utc::now(),
        }
    }

    #[tokio::test]
    async fn ouvre_une_session_avec_le_bon_mot_de_passe() {
        let auth = Arc::new(InMemoryAuth::with("une phrase de passe", []));
        let output = use_case(auth.clone())
            .handle(command("une phrase de passe"))
            .await
            .unwrap();
        assert_eq!(auth.session_count(), 1);
        // Seule l'empreinte du jeton est conservée.
        assert!(
            auth.read_session(&auth::hash_token(&output.token))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn un_mauvais_mot_de_passe_est_refuse_sans_dire_ce_qui_manque() {
        let auth = Arc::new(InMemoryAuth::with("une phrase de passe", []));
        let result = use_case(auth.clone()).handle(command("autre chose")).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(m)) if m == "Mot de passe incorrect."));
        assert_eq!(auth.session_count(), 0);
    }

    #[tokio::test]
    async fn avant_la_configuration_il_n_y_a_rien_a_ouvrir() {
        let result = use_case(Arc::new(InMemoryAuth::default()))
            .handle(command("x"))
            .await;
        assert!(
            matches!(result, Err(UseCaseError::Refusal(m)) if m.contains("pas encore configuré"))
        );
    }

    #[tokio::test]
    async fn cinq_echecs_de_suite_ferment_la_porte_meme_au_bon_mot_de_passe() {
        let auth = Arc::new(InMemoryAuth::with("une phrase de passe", []));
        let use_case = use_case(auth.clone());
        for _ in 0..Throttle::FREE_ATTEMPTS {
            let _ = use_case.handle(command("faux")).await;
        }
        let result = use_case.handle(command("une phrase de passe")).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(m)) if m.contains("Trop d'essais")));
        assert_eq!(auth.session_count(), 0);
    }

    #[tokio::test]
    async fn les_echecs_d_un_client_ne_ferment_pas_la_porte_a_un_autre() {
        let auth = Arc::new(InMemoryAuth::with("une phrase de passe", []));
        let use_case = use_case(auth.clone());
        for _ in 0..Throttle::FREE_ATTEMPTS {
            let _ = use_case.handle(command("faux")).await;
        }
        let owner = Command {
            client: "198.51.100.9".into(),
            ..command("une phrase de passe")
        };
        assert!(use_case.handle(owner).await.is_ok());
    }
}
