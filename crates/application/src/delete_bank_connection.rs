use std::sync::Arc;

use bank::{BankAggregator, BankConnectionProvider};
use shared::BankConnectionId;

use crate::UseCaseError;

pub struct DeleteBankConnection {
    pub aggregator: Option<Arc<dyn BankAggregator>>,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
}

pub struct Command {
    pub connection_id: BankConnectionId,
}

impl DeleteBankConnection {
    /// Retire le consentement. Les opérations déjà lues restent dans leurs comptes.
    pub async fn handle(&self, command: Command) -> Result<(), UseCaseError> {
        let connection = self
            .bank_provider
            .read_connections()
            .await?
            .into_iter()
            .find(|c| c.id == command.connection_id)
            .ok_or_else(|| {
                UseCaseError::refusal(
                    "Cette connexion n'existe plus : elle a peut-être déjà été retirée.",
                )
            })?;
        // Au mieux : si la banque est injoignable, le consentement s'éteindra à son terme.
        if let Some(aggregator) = &self.aggregator
            && let Err(e) = aggregator.close_session(&connection.session_id).await
        {
            tracing::warn!(error = ?e, "Fermeture de la session bancaire impossible.");
        }
        self.bank_provider.delete_connection(&connection.id).await?;
        Ok(())
    }
}
