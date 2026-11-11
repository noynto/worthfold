//! Implémentations en mémoire des providers, pour les tests des cas d'usage.
//! Ajoutée seulement en `[dev-dependencies]` : jamais dans un binaire.

use std::sync::Mutex;

use asset::{Asset, AssetProvider};
use async_trait::async_trait;
use shared::{AssetId, ValuationId};
use valuation::{Valuation, ValuationProvider};

#[derive(Default)]
pub struct InMemoryAssets(Mutex<Vec<Asset>>);

impl InMemoryAssets {
    pub fn with(assets: impl IntoIterator<Item = Asset>) -> Self {
        Self(Mutex::new(assets.into_iter().collect()))
    }
}

#[async_trait]
impl AssetProvider for InMemoryAssets {
    async fn read_all(&self) -> anyhow::Result<Vec<Asset>> {
        Ok(self.0.lock().unwrap().clone())
    }

    async fn read(&self, id: &AssetId) -> anyhow::Result<Option<Asset>> {
        Ok(self.0.lock().unwrap().iter().find(|a| &a.id == id).cloned())
    }

    async fn write(&self, asset: &Asset) -> anyhow::Result<()> {
        let mut assets = self.0.lock().unwrap();
        assets.retain(|a| a.id != asset.id);
        assets.push(asset.clone());
        Ok(())
    }

    async fn delete(&self, id: &AssetId) -> anyhow::Result<()> {
        self.0.lock().unwrap().retain(|a| &a.id != id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryValuations(Mutex<Vec<Valuation>>);

impl InMemoryValuations {
    pub fn with(valuations: impl IntoIterator<Item = Valuation>) -> Self {
        Self(Mutex::new(valuations.into_iter().collect()))
    }
}

#[async_trait]
impl ValuationProvider for InMemoryValuations {
    async fn read_all(&self) -> anyhow::Result<Vec<Valuation>> {
        Ok(self.0.lock().unwrap().clone())
    }

    async fn write(&self, valuation: &Valuation) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(valuation.clone());
        Ok(())
    }

    async fn read(&self, id: &ValuationId) -> anyhow::Result<Option<Valuation>> {
        Ok(self.0.lock().unwrap().iter().find(|v| &v.id == id).cloned())
    }

    async fn delete(&self, id: &ValuationId) -> anyhow::Result<()> {
        self.0.lock().unwrap().retain(|v| &v.id != id);
        Ok(())
    }

    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        self.0.lock().unwrap().retain(|v| &v.asset_id != asset_id);
        Ok(())
    }
}
