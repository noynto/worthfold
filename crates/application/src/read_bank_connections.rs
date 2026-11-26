use std::sync::Arc;

use asset::{Asset, AssetKind, AssetProvider};
use bank::{BankAccount, BankConnection, BankConnectionProvider, RENEW_WARNING_DAYS};
use chrono::NaiveDate;

use crate::UseCaseError;

pub struct ReadBankConnections {
    /// Seule sa présence compte ici : la connexion est-elle configurée ?
    pub configured: bool,
    pub bank_provider: Arc<dyn BankConnectionProvider>,
    pub asset_provider: Arc<dyn AssetProvider>,
}

pub struct Query {
    pub today: NaiveDate,
}

pub struct ConnectionView {
    pub connection: BankConnection,
    /// Négatif une fois expiré.
    pub days_left: i64,
    /// Il reste peu de jours : à renouveler bientôt.
    pub renew_soon: bool,
    pub accounts: Vec<BankAccount>,
}

pub struct Output {
    pub configured: bool,
    pub connections: Vec<ConnectionView>,
    /// Les comptes bancaires de worthfold, à rattacher aux comptes de la banque.
    pub assets: Vec<Asset>,
}

impl ReadBankConnections {
    pub async fn handle(&self, query: Query) -> Result<Output, UseCaseError> {
        let connections = self.bank_provider.read_connections().await?;
        let accounts = self.bank_provider.read_accounts().await?;
        let assets = self
            .asset_provider
            .read_all()
            .await?
            .into_iter()
            .filter(|a| a.kind == AssetKind::BankAccount)
            .collect();
        Ok(Output {
            configured: self.configured,
            connections: connections
                .into_iter()
                .map(|connection| {
                    let days_left = connection.days_left(query.today);
                    ConnectionView {
                        renew_soon: days_left <= RENEW_WARNING_DAYS,
                        days_left,
                        accounts: accounts
                            .iter()
                            .filter(|a| a.connection_id == connection.id)
                            .cloned()
                            .collect(),
                        connection,
                    }
                })
                .collect(),
            assets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::BankConnectionId;
    use testing::{InMemoryAssets, InMemoryBank};

    #[tokio::test]
    async fn signale_les_consentements_proches_de_leur_fin() {
        let connection = |name: &str, valid_until: (u32, u32)| BankConnection {
            id: BankConnectionId::generate(),
            bank_name: name.into(),
            country: "FR".into(),
            session_id: "s".into(),
            valid_until: NaiveDate::from_ymd_opt(2026, valid_until.0, valid_until.1).unwrap(),
            created_on: NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
        };
        let output = ReadBankConnections {
            configured: true,
            bank_provider: Arc::new(InMemoryBank::with(
                vec![connection("Loin", (12, 31)), connection("Proche", (12, 1))],
                vec![],
            )),
            asset_provider: Arc::new(InMemoryAssets::default()),
        }
        .handle(Query {
            today: NaiveDate::from_ymd_opt(2026, 11, 25).unwrap(),
        })
        .await
        .unwrap();
        let soon: Vec<_> = output.connections.iter().map(|c| c.renew_soon).collect();
        assert_eq!(soon, [false, true]);
        assert_eq!(output.connections[1].days_left, 6);
    }
}
