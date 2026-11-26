use std::sync::Arc;

use auth::{AuthProvider, Credential, PasswordHasher, Session, Throttle, check_new_password};
use chrono::{DateTime, Utc};

use crate::UseCaseError;
use crate::session::new_token;

/// Première configuration : choisir le mot de passe. Seule une personne qui lit la console du serveur
/// (où le code est affiché) peut le faire, et seulement tant qu'aucun mot de passe n'existe.
pub struct SetupPassword {
    pub auth_provider: Arc<dyn AuthProvider>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub throttle: Arc<Throttle>,
    /// Le code de configuration, tiré au démarrage et affiché dans la console.
    pub setup_code: Arc<str>,
}

pub struct Command {
    pub setup_code: String,
    pub password: String,
    pub confirmation: String,
    /// Qui essaie (son adresse IP) : le frein des essais est propre à chaque client.
    pub client: String,
    pub now: DateTime<Utc>,
}

pub struct Output {
    /// Le jeton de la session ouverte : l'utilisateur est connecté dès la configuration.
    pub token: String,
}

/// Le code sans espaces ni tirets, en majuscules.
fn normalize(code: &str) -> String {
    code.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_uppercase()
}

impl SetupPassword {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        if self.auth_provider.read_credential().await?.is_some() {
            return Err(UseCaseError::refusal(
                "worthfold est déjà configuré : connecte-toi avec ton mot de passe.",
            ));
        }
        if let Err(seconds) = self.throttle.check(&command.client, command.now) {
            return Err(UseCaseError::refusal(crate::session::too_many_attempts(
                seconds,
            )));
        }
        // Le code se recopie depuis la console : la casse, les espaces et les tirets ne comptent pas.
        if normalize(&command.setup_code) != normalize(&self.setup_code) {
            self.throttle.record_failure(&command.client, command.now);
            return Err(UseCaseError::refusal(
                "Le code de configuration n'est pas le bon : il est affiché dans la console où worthfold a démarré.",
            ));
        }
        check_new_password(&command.password, &command.confirmation)
            .map_err(UseCaseError::refusal)?;
        let password_hash = self.hasher.hash(&command.password)?;
        self.auth_provider
            .write_credential(&Credential { password_hash })
            .await?;
        let token = new_token();
        self.auth_provider
            .write_session(&Session::open(&token, command.now))
            .await?;
        self.throttle.record_success(&command.client);
        Ok(Output { token })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{InMemoryAuth, PlainHasher};

    fn use_case(auth: Arc<InMemoryAuth>) -> SetupPassword {
        SetupPassword {
            auth_provider: auth,
            hasher: Arc::new(PlainHasher),
            throttle: Arc::default(),
            setup_code: "CODE-1234".into(),
        }
    }

    fn command(code: &str, password: &str, confirmation: &str) -> Command {
        Command {
            setup_code: code.into(),
            password: password.into(),
            confirmation: confirmation.into(),
            client: "203.0.113.7".into(),
            now: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn configure_le_mot_de_passe_et_ouvre_une_session() {
        let auth = Arc::new(InMemoryAuth::default());
        let output = use_case(auth.clone())
            .handle(command(
                "CODE1234",
                "une phrase de passe",
                "une phrase de passe",
            ))
            .await
            .unwrap();
        assert!(output.token.len() >= 64);
        assert!(auth.read_credential().await.unwrap().is_some());
        assert_eq!(auth.session_count(), 1);
        // Le mot de passe n'est conservé que haché.
        let stored = auth.read_credential().await.unwrap().unwrap().password_hash;
        assert_ne!(stored, "une phrase de passe");
    }

    #[tokio::test]
    async fn refuse_un_mauvais_code_un_mot_de_passe_faible_ou_une_seconde_configuration() {
        let auth = Arc::new(InMemoryAuth::default());
        let use_case = use_case(auth.clone());
        for (code, password, confirmation) in [
            ("MAUVAIS", "une phrase de passe", "une phrase de passe"),
            ("CODE1234", "court", "court"),
            ("CODE1234", "une phrase de passe", "une autre phrase"),
        ] {
            assert!(matches!(
                use_case.handle(command(code, password, confirmation)).await,
                Err(UseCaseError::Refusal(_))
            ));
        }
        assert!(auth.read_credential().await.unwrap().is_none());

        use_case
            .handle(command(
                "CODE1234",
                "une phrase de passe",
                "une phrase de passe",
            ))
            .await
            .unwrap();
        let again = use_case
            .handle(command(
                "CODE1234",
                "un autre mot de passe",
                "un autre mot de passe",
            ))
            .await;
        assert!(matches!(again, Err(UseCaseError::Refusal(m)) if m.contains("déjà configuré")));
    }

    #[tokio::test]
    async fn des_codes_faux_a_repetition_ferment_la_porte() {
        let use_case = use_case(Arc::new(InMemoryAuth::default()));
        for _ in 0..Throttle::FREE_ATTEMPTS {
            let _ = use_case
                .handle(command(
                    "MAUVAIS",
                    "une phrase de passe",
                    "une phrase de passe",
                ))
                .await;
        }
        // Même le bon code est refusé tant que la porte est fermée.
        let result = use_case
            .handle(command(
                "CODE1234",
                "une phrase de passe",
                "une phrase de passe",
            ))
            .await;
        assert!(matches!(result, Err(UseCaseError::Refusal(m)) if m.contains("Trop d'essais")));
    }
}
