use std::sync::Arc;

use bank::{BankAggregator, BankConnectionProvider, CONSENT_DAYS, PendingAuthorization};
use chrono::{Days, NaiveDate};

use crate::UseCaseError;

pub struct StartBankConnection {
    /// Absent tant que la connexion bancaire n'est pas configurée.
    pub aggregator: Option<Arc<dyn BankAggregator>>,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
}

pub struct Command {
    pub bank_name: String,
    /// Code pays à deux lettres (« FR »).
    pub country: String,
    pub today: NaiveDate,
}

pub struct Output {
    /// L'adresse de la banque où envoyer l'utilisateur donner son consentement.
    pub url: String,
}

impl StartBankConnection {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        let aggregator = self
            .aggregator
            .as_ref()
            .ok_or_else(|| UseCaseError::refusal(crate::BANK_NOT_CONFIGURED))?;
        let bank_name = command.bank_name.trim().to_owned();
        if bank_name.is_empty() {
            return Err(UseCaseError::refusal("Le nom de la banque est nécessaire."));
        }
        let country = command.country.trim().to_uppercase();
        if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(UseCaseError::refusal(
                "Le pays se note avec deux lettres, par exemple FR.",
            ));
        }
        let state = shared::random_token();
        let valid_until = command.today + Days::new(CONSENT_DAYS);
        let url = aggregator
            .authorize(&bank_name, &country, valid_until, &state)
            .await
            .map_err(|e| {
                tracing::error!(error = ?e, "Démarrage du consentement bancaire refusé.");
                UseCaseError::refusal(
                    "La banque n'a pas accepté la demande. Vérifie son nom exact et le pays.",
                )
            })?;
        self.bank_provider
            .write_pending(&PendingAuthorization {
                state,
                bank_name,
                country,
                created_on: command.today,
            })
            .await?;
        Ok(Output { url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{FakeAggregator, InMemoryBank};

    fn use_case() -> (StartBankConnection, Arc<InMemoryBank>) {
        let bank = Arc::new(InMemoryBank::default());
        (
            StartBankConnection {
                aggregator: Some(Arc::new(FakeAggregator::default())),
                bank_provider: bank.clone(),
            },
            bank,
        )
    }

    fn command(name: &str, country: &str) -> Command {
        Command {
            bank_name: name.into(),
            country: country.into(),
            today: NaiveDate::from_ymd_opt(2026, 11, 25).unwrap(),
        }
    }

    #[tokio::test]
    async fn rend_l_adresse_de_la_banque_et_retient_l_etat() {
        let (use_case, bank) = use_case();
        let output = use_case
            .handle(command(" Mock ASPSP ", "fr"))
            .await
            .unwrap();
        let state = output.url.split("state=").nth(1).unwrap();
        assert!(output.url.contains("/FR/Mock ASPSP?"));
        let pending = bank.take_pending(state).await.unwrap().unwrap();
        assert_eq!(
            (pending.bank_name.as_str(), pending.country.as_str()),
            ("Mock ASPSP", "FR")
        );
    }

    #[tokio::test]
    async fn refuse_un_nom_vide_un_pays_invalide_et_l_absence_de_configuration() {
        let (use_case, _) = use_case();
        assert!(matches!(
            use_case.handle(command("  ", "FR")).await,
            Err(UseCaseError::Refusal(_))
        ));
        assert!(matches!(
            use_case.handle(command("B", "France")).await,
            Err(UseCaseError::Refusal(_))
        ));
        let unconfigured = StartBankConnection {
            aggregator: None,
            bank_provider: Arc::new(InMemoryBank::default()),
        };
        let Err(UseCaseError::Refusal(message)) = unconfigured.handle(command("B", "FR")).await
        else {
            panic!("refus attendu");
        };
        assert!(message.contains("configur"), "{message}");
    }
}
