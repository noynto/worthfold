use std::sync::Arc;

use auth::{AuthProvider, Credential, PasswordHasher, Session, Throttle, check_new_password};
use chrono::{DateTime, Utc};

use crate::UseCaseError;
use crate::session::{new_token, too_many_attempts};

pub struct ChangePassword {
    pub auth_provider: Arc<dyn AuthProvider>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub throttle: Arc<Throttle>,
}

pub struct Command {
    pub current: String,
    pub password: String,
    pub confirmation: String,
    /// Qui essaie (son adresse IP) : le frein des essais est propre à chaque client.
    pub client: String,
    pub now: DateTime<Utc>,
}

pub struct Output {
    /// Toutes les sessions sont fermées, y compris celle-ci : voici celle qui la remplace.
    pub token: String,
}

impl ChangePassword {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let Some(credential) = self.auth_provider.read_credential().await? else {
            return Err(UseCaseError::refusal(
                "worthfold n'est pas encore configuré : choisis d'abord un mot de passe.",
            ));
        };
        if let Err(seconds) = self.throttle.check(&command.client, command.now) {
            return Err(UseCaseError::refusal(too_many_attempts(seconds)));
        }
        // Le mot de passe actuel est redemandé : une session ouverte oubliée ne suffit pas à le changer.
        if !self
            .hasher
            .verify(&command.current, &credential.password_hash)?
        {
            self.throttle.record_failure(&command.client, command.now);
            return Err(UseCaseError::refusal(
                "Le mot de passe actuel n'est pas le bon.",
            ));
        }
        check_new_password(&command.password, &command.confirmation)
            .map_err(UseCaseError::refusal)?;
        self.throttle.record_success(&command.client);
        let password_hash = self.hasher.hash(&command.password)?;
        self.auth_provider
            .write_credential(&Credential { password_hash })
            .await?;
        // Une session copiée ou oubliée ailleurs ne doit pas survivre au changement de mot de passe.
        self.auth_provider.delete_all_sessions().await?;
        let token = new_token();
        self.auth_provider
            .write_session(&Session::open(&token, command.now))
            .await?;
        Ok(Output { token })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{InMemoryAuth, PlainHasher};

    fn command(current: &str, password: &str, confirmation: &str) -> Command {
        Command {
            current: current.into(),
            password: password.into(),
            confirmation: confirmation.into(),
            client: "203.0.113.7".into(),
            now: Utc::now(),
        }
    }

    #[tokio::test]
    async fn change_le_mot_de_passe_et_ferme_les_autres_sessions() {
        let now = Utc::now();
        let auth = Arc::new(InMemoryAuth::with(
            "ancienne phrase de passe",
            [Session::open("ailleurs", now)],
        ));
        let use_case = ChangePassword {
            auth_provider: auth.clone(),
            hasher: Arc::new(PlainHasher),
            throttle: Arc::default(),
        };

        assert!(matches!(
            use_case
                .handle(command("faux", "nouvelle phrase de passe", "nouvelle phrase de passe"))
                .await,
            Err(UseCaseError::Refusal(m)) if m.contains("actuel")
        ));
        assert!(matches!(
            use_case
                .handle(command("ancienne phrase de passe", "court", "court"))
                .await,
            Err(UseCaseError::Refusal(_))
        ));
        assert_eq!(auth.session_count(), 1);

        let output = use_case
            .handle(command(
                "ancienne phrase de passe",
                "nouvelle phrase de passe",
                "nouvelle phrase de passe",
            ))
            .await
            .unwrap();
        // L'ancienne session est fermée, une seule reste : la nouvelle.
        assert_eq!(auth.session_count(), 1);
        assert!(
            auth.read_session(&auth::hash_token("ailleurs"))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            auth.read_session(&auth::hash_token(&output.token))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            PlainHasher
                .verify(
                    "nouvelle phrase de passe",
                    &auth.read_credential().await.unwrap().unwrap().password_hash
                )
                .unwrap()
        );
    }
}
