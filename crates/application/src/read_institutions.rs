use std::sync::Arc;

use asset::AssetProvider;
use institution::{Institution, InstitutionProvider};

use crate::UseCaseError;

pub struct ReadInstitutions {
    pub institution_provider: Arc<dyn InstitutionProvider>,
    pub asset_provider: Arc<dyn AssetProvider>,
}

pub struct Query;

pub struct Output {
    /// Par ordre alphabétique.
    pub institutions: Vec<Line>,
}

pub struct Line {
    pub institution: Institution,
    /// Le nombre de comptes bancaires qui s'y rattachent.
    pub accounts: usize,
}

impl ReadInstitutions {
    pub async fn handle(&self, _query: Query) -> Result<Output, UseCaseError> {
        let assets = self.asset_provider.read_all().await?;
        let institutions = self
            .institution_provider
            .read_all()
            .await?
            .into_iter()
            .map(|institution| Line {
                accounts: assets
                    .iter()
                    .filter(|a| a.bank_name.as_deref() == Some(institution.name.as_str()))
                    .count(),
                institution,
            })
            .collect();
        Ok(Output { institutions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::{Asset, AssetKind};
    use institution::CsvFormat;
    use shared::Currency;
    use testing::{InMemoryAssets, InMemoryInstitutions};

    #[tokio::test]
    async fn compte_les_comptes_de_chaque_banque() {
        let bank = |name: &str| Institution {
            name: name.into(),
            csv: CsvFormat::suggested(),
            pdf: None,
        };
        let account = |id: &str, bank: Option<&str>| Asset {
            id: id.into(),
            name: id.into(),
            kind: AssetKind::BankAccount,
            currency: Currency::Eur,
            bank_name: bank.map(Into::into),
            account_number: None,
        };
        let use_case = ReadInstitutions {
            institution_provider: Arc::new(InMemoryInstitutions::with([bank("B"), bank("A")])),
            asset_provider: Arc::new(InMemoryAssets::with([
                account("x", Some("A")),
                account("y", Some("A")),
                account("z", None),
            ])),
        };
        let lines = use_case.handle(Query).await.unwrap().institutions;
        let summary: Vec<(&str, usize)> = lines
            .iter()
            .map(|l| (l.institution.name.as_str(), l.accounts))
            .collect();
        assert_eq!(summary, vec![("A", 2), ("B", 0)]);
    }
}
