use anyhow::Context;
use asset::{Asset, AssetProvider};
use async_trait::async_trait;
use shared::AssetId;
use sqlx::{FromRow, PgPool};

pub struct PersistedAssets {
    pool: PgPool,
}

impl PersistedAssets {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(FromRow)]
struct AssetRow {
    id: String,
    name: String,
    kind: String,
    currency: String,
    bank_name: Option<String>,
    account_number: Option<String>,
}

impl TryFrom<AssetRow> for Asset {
    type Error = anyhow::Error;

    fn try_from(row: AssetRow) -> anyhow::Result<Self> {
        Ok(Asset {
            kind: row.kind.parse().map_err(anyhow::Error::msg)?,
            currency: row.currency.parse()?,
            id: AssetId::from(row.id),
            name: row.name,
            bank_name: row.bank_name,
            account_number: row.account_number,
        })
    }
}

#[async_trait]
impl AssetProvider for PersistedAssets {
    async fn read_all(&self) -> anyhow::Result<Vec<Asset>> {
        sqlx::query_as::<_, AssetRow>(
            "SELECT id, name, kind, currency, bank_name, account_number FROM assets ORDER BY name COLLATE \"C\"",
        )
        .fetch_all(&self.pool)
        .await
        .context("Impossible de lire les actifs.")?
        .into_iter()
        .map(Asset::try_from)
        .collect()
    }

    async fn read(&self, id: &AssetId) -> anyhow::Result<Option<Asset>> {
        sqlx::query_as::<_, AssetRow>(
            "SELECT id, name, kind, currency, bank_name, account_number FROM assets WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("Impossible de lire l'actif {id}."))?
        .map(Asset::try_from)
        .transpose()
    }

    async fn write(&self, asset: &Asset) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO assets (id, name, kind, currency, bank_name, account_number) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, kind = excluded.kind, currency = excluded.currency,
             bank_name = excluded.bank_name, account_number = excluded.account_number",
        )
        .bind(asset.id.as_str())
        .bind(&asset.name)
        .bind(asset.kind.code())
        .bind(asset.currency.code())
        .bind(&asset.bank_name)
        .bind(&asset.account_number)
        .execute(&self.pool)
        .await
        .with_context(|| format!("Impossible d'écrire l'actif {}.", asset.id))?;
        Ok(())
    }

    async fn delete(&self, id: &AssetId) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM assets WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .with_context(|| format!("Impossible de supprimer l'actif {id}."))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedInstitutions;
    use crate::persistence::testdb::database;
    use asset::AssetKind;
    use institution::{CsvFormat, Institution, InstitutionProvider};
    use shared::Currency;

    #[tokio::test]
    async fn la_banque_et_le_numero_font_l_aller_retour_et_peuvent_etre_retires() {
        let Some(db) = database().await else { return };
        let assets = PersistedAssets::new(db.pool.clone());
        // La banque d'un compte doit exister : la base le garantit.
        PersistedInstitutions::new(db.pool.clone())
            .write(&Institution {
                name: "Ma banque".into(),
                csv: CsvFormat::suggested(),
                pdf: None,
            })
            .await
            .unwrap();
        let mut account = Asset {
            id: AssetId::generate(),
            name: "Courant".into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: Some("Ma banque".into()),
            account_number: Some("FR1420041010050500013M02606".into()),
        };
        assets.write(&account).await.unwrap();
        assert_eq!(
            assets.read(&account.id).await.unwrap(),
            Some(account.clone())
        );

        account.bank_name = None;
        account.account_number = None;
        assets.write(&account).await.unwrap();
        assert_eq!(assets.read_all().await.unwrap(), vec![account]);
        db.drop_db().await;
    }
}
