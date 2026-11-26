use std::sync::Arc;

use asset::{Asset, AssetKind, AssetProvider};
use shared::{AssetId, Currency};
use valuation::ValuationProvider;

use crate::bank_details::bank_details;
use crate::{NOT_FOUND, UseCaseError};

pub struct UpdateAsset {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
}

pub struct Command {
    pub asset_id: AssetId,
    pub name: String,
    pub kind: AssetKind,
    pub currency: Currency,
    /// Banque et numéro de compte, tels que saisis : retenus pour un compte bancaire seulement.
    pub bank_name: String,
    pub account_number: String,
}

pub struct Output {
    pub asset: Asset,
}

impl UpdateAsset {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        // 1. Vérification de la commande.
        let name = Asset::validate_name(&command.name).map_err(UseCaseError::Refusal)?;
        let (bank_name, account_number) =
            bank_details(command.kind, &command.bank_name, &command.account_number)?;
        // 2. Résolution de l'actif.
        let mut asset = self
            .asset_provider
            .read(&command.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        // 3. Règles : dès la première valorisation, la devise et le sens (avoir ou dette) sont figés,
        //    sinon l'historique changerait de devise ou de signe.
        let changes_currency = command.currency != asset.currency;
        let changes_direction = command.kind.is_liability() != asset.kind.is_liability();
        if changes_currency || changes_direction {
            let valued = self
                .valuation_provider
                .read_all()
                .await?
                .iter()
                .any(|v| v.asset_id == asset.id);
            if valued && changes_currency {
                return Err(UseCaseError::refusal(format!(
                    "La devise de « {} » ne peut plus changer : l'actif a déjà des valorisations en {}.",
                    asset.name, asset.currency
                )));
            }
            if valued && changes_direction {
                return Err(UseCaseError::refusal(format!(
                    "« {} » a déjà des valorisations : il ne peut plus passer d'un avoir à un prêt, ni l'inverse.",
                    asset.name
                )));
            }
        }
        // 4. Écriture.
        asset.name = name;
        asset.kind = command.kind;
        asset.currency = command.currency;
        asset.bank_name = bank_name;
        asset.account_number = account_number;
        self.asset_provider.write(&asset).await?;
        Ok(Output { asset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::Money;
    use testing::{InMemoryAssets, InMemoryValuations};
    use valuation::Valuation;

    fn livret() -> Asset {
        Asset {
            id: "a".into(),
            name: "Livret".into(),
            kind: AssetKind::SavingsBook,
            currency: Currency::Eur,
            bank_name: None,
            account_number: None,
        }
    }

    fn command(name: &str, currency: Currency) -> Command {
        Command {
            asset_id: "a".into(),
            name: name.into(),
            kind: AssetKind::BankAccount,
            currency,
            bank_name: String::new(),
            account_number: String::new(),
        }
    }

    #[tokio::test]
    async fn modifie_le_nom_et_le_type_d_un_actif_sans_historique() {
        let assets = Arc::new(InMemoryAssets::with([livret()]));
        let use_case = UpdateAsset {
            asset_provider: assets.clone(),
            valuation_provider: Arc::new(InMemoryValuations::default()),
        };

        use_case
            .handle(command("Compte courant", Currency::Eur))
            .await
            .unwrap();

        let saved = assets.read(&"a".into()).await.unwrap().unwrap();
        assert_eq!(
            (saved.name.as_str(), saved.kind, saved.currency),
            ("Compte courant", AssetKind::BankAccount, Currency::Eur)
        );
    }

    #[tokio::test]
    async fn refuse_de_changer_le_sens_d_un_actif_valorise() {
        let valuation = Valuation {
            id: shared::ValuationId::generate(),
            asset_id: "a".into(),
            value: Money::new(Decimal::ONE, Currency::Eur),
            valued_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
        };
        let use_case = UpdateAsset {
            asset_provider: Arc::new(InMemoryAssets::with([livret()])),
            valuation_provider: Arc::new(InMemoryValuations::with([valuation])),
        };

        // Un avoir valorisé ne devient pas un prêt.
        let to_loan = Command {
            kind: AssetKind::Loan,
            ..command("Livret", Currency::Eur)
        };
        assert!(matches!(
            use_case.handle(to_loan).await,
            Err(UseCaseError::Refusal(_))
        ));
        // Le nom, lui, reste modifiable.
        assert!(
            use_case
                .handle(command("Livret A", Currency::Eur))
                .await
                .is_ok()
        );
    }
}
