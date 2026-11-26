use anyhow::Context;
use async_trait::async_trait;
use bank::{BankAccount, BankConnection, BankConnectionProvider, PendingAuthorization};
use chrono::NaiveDate;
use shared::{AssetId, BankConnectionId};
use sqlx::{FromRow, PgPool};

pub struct PersistedBank {
    pool: PgPool,
}

impl PersistedBank {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct ConnectionRow {
    id: String,
    bank_name: String,
    country: String,
    session_id: String,
    valid_until: NaiveDate,
    created_on: NaiveDate,
}

#[derive(FromRow)]
struct AccountRow {
    uid: String,
    connection_id: String,
    identity: String,
    label: String,
    currency: String,
    asset_id: Option<String>,
    last_synced_on: Option<NaiveDate>,
}

#[derive(FromRow)]
struct PendingRow {
    state: String,
    bank_name: String,
    country: String,
    created_on: NaiveDate,
}

#[async_trait]
impl BankConnectionProvider for PersistedBank {
    async fn read_connections(&self) -> anyhow::Result<Vec<BankConnection>> {
        let rows: Vec<ConnectionRow> = sqlx::query_as(
            "SELECT id, bank_name, country, session_id, valid_until, created_on \
             FROM bank_connections ORDER BY created_on, id",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les connexions bancaires.")?;
        rows.into_iter()
            .map(|r| {
                Ok(BankConnection {
                    id: BankConnectionId::from(r.id),
                    bank_name: r.bank_name,
                    country: r.country,
                    session_id: r.session_id,
                    valid_until: r.valid_until,
                    created_on: r.created_on,
                })
            })
            .collect()
    }

    async fn read_accounts(&self) -> anyhow::Result<Vec<BankAccount>> {
        let rows: Vec<AccountRow> = sqlx::query_as(
            "SELECT uid, connection_id, identity, label, currency, asset_id, last_synced_on \
             FROM bank_accounts ORDER BY label, uid",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les comptes de la banque.")?;
        rows.into_iter()
            .map(|r| {
                Ok(BankAccount {
                    uid: r.uid,
                    connection_id: BankConnectionId::from(r.connection_id),
                    identity: r.identity,
                    label: r.label,
                    currency: r.currency.parse()?,
                    asset_id: r.asset_id.map(AssetId::from),
                    last_synced_on: r.last_synced_on,
                })
            })
            .collect()
    }

    async fn write_connection(
        &self,
        connection: &BankConnection,
        accounts: &[BankAccount],
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO bank_connections (id, bank_name, country, session_id, valid_until, created_on) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(connection.id.as_str())
        .bind(&connection.bank_name)
        .bind(&connection.country)
        .bind(&connection.session_id)
        .bind(connection.valid_until)
        .bind(connection.created_on)
        .execute(&mut *tx)
        .await
        .context("Impossible d'enregistrer la connexion bancaire.")?;
        for account in accounts {
            sqlx::query(
                "INSERT INTO bank_accounts (uid, connection_id, identity, label, currency, asset_id, last_synced_on) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(&account.uid)
            .bind(account.connection_id.as_str())
            .bind(&account.identity)
            .bind(&account.label)
            .bind(account.currency.code())
            .bind(account.asset_id.as_ref().map(AssetId::as_str))
            .bind(account.last_synced_on)
            .execute(&mut *tx)
            .await
            .context("Impossible d'enregistrer un compte de la banque.")?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn delete_connection(&self, id: &BankConnectionId) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM bank_accounts WHERE connection_id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM bank_connections WHERE id = $1")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn link_account(&self, uid: &str, asset_id: Option<&AssetId>) -> anyhow::Result<()> {
        sqlx::query("UPDATE bank_accounts SET asset_id = $1 WHERE uid = $2")
            .bind(asset_id.map(AssetId::as_str))
            .bind(uid)
            .execute(&self.pool)
            .await
            .context("Impossible de rattacher le compte.")?;
        Ok(())
    }

    async fn set_synced(&self, uid: &str, date: NaiveDate) -> anyhow::Result<()> {
        sqlx::query("UPDATE bank_accounts SET last_synced_on = $1 WHERE uid = $2")
            .bind(date)
            .bind(uid)
            .execute(&self.pool)
            .await
            .context("Impossible d'enregistrer la date de synchronisation.")?;
        Ok(())
    }

    async fn unlink_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        sqlx::query("UPDATE bank_accounts SET asset_id = NULL WHERE asset_id = $1")
            .bind(asset_id.as_str())
            .execute(&self.pool)
            .await
            .context("Impossible de détacher les comptes de la banque.")?;
        Ok(())
    }

    async fn write_pending(&self, pending: &PendingAuthorization) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO bank_pending (state, bank_name, country, created_on) VALUES ($1, $2, $3, $4)",
        )
        .bind(&pending.state)
        .bind(&pending.bank_name)
        .bind(&pending.country)
        .bind(pending.created_on)
        .execute(&self.pool)
        .await
        .context("Impossible d'enregistrer la demande de consentement.")?;
        Ok(())
    }

    /// Atomique : une seule requête lit et retire la demande, si bien qu'un état rejoué en même temps
    /// ne sert qu'une fois.
    async fn take_pending(&self, state: &str) -> anyhow::Result<Option<PendingAuthorization>> {
        let row: Option<PendingRow> = sqlx::query_as(
            "DELETE FROM bank_pending WHERE state = $1 RETURNING state, bank_name, country, created_on",
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .context("Impossible de lire la demande de consentement.")?;
        Ok(row.map(|r| PendingAuthorization {
            state: r.state,
            bank_name: r.bank_name,
            country: r.country,
            created_on: r.created_on,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedAssets;
    use crate::persistence::testdb::database;
    use asset::{Asset, AssetKind, AssetProvider};
    use shared::Currency;

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    #[tokio::test]
    async fn enregistre_rattache_et_supprime_une_connexion() {
        let Some(db) = database().await else { return };
        let pool = db.pool.clone();
        let assets = PersistedAssets::new(pool.clone());
        let asset = Asset {
            id: AssetId::generate(),
            name: "Compte".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        };
        assets.write(&asset).await.unwrap();
        let bank = PersistedBank::new(pool);
        let connection = BankConnection {
            id: BankConnectionId::generate(),
            bank_name: "Banque".into(),
            country: "FR".into(),
            session_id: "s".into(),
            valid_until: day(2, 23),
            created_on: day(11, 25),
        };
        let account = BankAccount {
            uid: "u1".into(),
            connection_id: connection.id.clone(),
            identity: "i".into(),
            label: "Courant".into(),
            currency: Currency::Eur,
            asset_id: None,
            last_synced_on: None,
        };
        bank.write_connection(&connection, &[account])
            .await
            .unwrap();
        bank.link_account("u1", Some(&asset.id)).await.unwrap();
        bank.set_synced("u1", day(11, 25)).await.unwrap();
        assert_eq!(
            bank.read_connections().await.unwrap(),
            vec![connection.clone()]
        );
        let read = bank.read_accounts().await.unwrap();
        assert_eq!(
            (read[0].asset_id.clone(), read[0].last_synced_on),
            (Some(asset.id.clone()), Some(day(11, 25)))
        );

        // Détacher avant de supprimer l'actif : la base refuse un rattachement orphelin.
        bank.unlink_asset(&asset.id).await.unwrap();
        assert_eq!(bank.read_accounts().await.unwrap()[0].asset_id, None);

        let pending = PendingAuthorization {
            state: "abc".into(),
            bank_name: "Banque".into(),
            country: "FR".into(),
            created_on: day(11, 25),
        };
        bank.write_pending(&pending).await.unwrap();
        assert_eq!(bank.take_pending("abc").await.unwrap(), Some(pending));
        assert_eq!(bank.take_pending("abc").await.unwrap(), None);

        bank.delete_connection(&connection.id).await.unwrap();
        assert!(bank.read_accounts().await.unwrap().is_empty());
        db.drop_db().await;
    }
}
