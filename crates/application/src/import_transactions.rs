use std::sync::Arc;

use asset::{AssetKind, AssetProvider};
use category::{CategoryProvider, matching_category};
use chrono::NaiveDate;
use institution::{InstitutionProvider, PdfReader};
use rust_decimal::Decimal;
use shared::{AssetId, Money, TransactionId, ValuationId};
use transaction::{Transaction, TransactionProvider, TransactionSource};
use valuation::{Valuation, ValuationProvider};

use crate::{NOT_FOUND, UseCaseError};

pub struct ImportTransactions {
    pub asset_provider: Arc<dyn AssetProvider>,
    pub transaction_provider: Arc<dyn TransactionProvider>,
    pub valuation_provider: Arc<dyn ValuationProvider>,
    pub category_provider: Arc<dyn CategoryProvider>,
    pub institution_provider: Arc<dyn InstitutionProvider>,
    pub pdf_reader: Arc<dyn PdfReader>,
}

pub struct Command {
    pub asset_id: AssetId,
    /// Le relevé de la banque (CSV ou PDF), tel quel : la banque du compte dit comment le décoder.
    pub statement: Vec<u8>,
    /// Le solde du compte juste après la dernière opération du relevé, si on le connaît.
    /// Il devient une valorisation du compte, donc il entre dans le patrimoine net.
    pub closing_balance: Option<Decimal>,
    /// La date du jour, fournie par l'appelant : un relevé ne peut pas contenir d'opération future.
    pub today: NaiveDate,
}

pub struct Output {
    /// Les opérations nouvelles, enregistrées.
    pub imported: usize,
    /// Les opérations que le compte connaissait déjà (relevé qui en recouvre un autre).
    pub already_known: usize,
    /// La valorisation créée à partir du solde indiqué, s'il y en avait un.
    pub balance: Option<Valuation>,
}

impl ImportTransactions {
    pub async fn handle(&self, command: Command) -> Result<Output, UseCaseError> {
        // 1. Résolution du compte : seul un compte bancaire a des opérations.
        let asset = self
            .asset_provider
            .read(&command.asset_id)
            .await?
            .ok_or_else(|| UseCaseError::refusal(NOT_FOUND))?;
        if asset.kind != AssetKind::BankAccount {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'est pas un compte bancaire : l'import de relevé ne concerne que les comptes bancaires.",
                asset.name
            )));
        }
        // 2. Lecture du fichier, selon le format de la banque du compte (règle pure, message prêt à afficher).
        let Some(bank_name) = &asset.bank_name else {
            return Err(UseCaseError::refusal(format!(
                "« {} » n'a pas de banque : choisis-la dans la page du compte, pour que je sache lire ses relevés.",
                asset.name
            )));
        };
        let institution = self
            .institution_provider
            .read(bank_name)
            .await?
            .ok_or_else(|| {
                UseCaseError::refusal(format!(
                    "La banque « {bank_name} » n'existe plus : choisis-en une autre pour ce compte."
                ))
            })?;
        let (lines, pdf_balance) = if command.statement.starts_with(b"%PDF") {
            let Some(format) = &institution.pdf else {
                return Err(UseCaseError::refusal(format!(
                    "« {} » n'a pas de format de relevé PDF : ajoute-le dans les réglages de la banque, ou importe un CSV.",
                    institution.name
                )));
            };
            let pages = self
                .pdf_reader
                .read(&command.statement)
                .map_err(UseCaseError::refusal)?;
            let statement =
                service::bank_pdf::parse(&pages, format).map_err(UseCaseError::refusal)?;
            (statement.lines, Some(statement.closing_balance))
        } else {
            let lines = service::bank_csv::parse(&command.statement, &institution.csv)
                .map_err(UseCaseError::refusal)?;
            (lines, None)
        };
        if let Some(future) = lines
            .iter()
            .map(|l| l.date)
            .filter(|d| *d > command.today)
            .max()
        {
            return Err(UseCaseError::refusal(format!(
                "Le relevé contient une opération datée du {}, dans le futur : vérifie le fichier.",
                future.format("%d/%m/%Y")
            )));
        }
        let last_date = lines.iter().map(|l| l.date).max();
        // 3. Ne garder que ce que le compte ne connaît pas encore.
        // Les opérations arrivent déjà classées quand une règle les reconnaît.
        let rules = self.category_provider.read_rules().await?;
        let incoming: Vec<Transaction> = lines
            .into_iter()
            .map(|line| Transaction {
                id: TransactionId::generate(),
                asset_id: asset.id.clone(),
                booked_on: line.date,
                amount: Money::new(line.amount, asset.currency),
                category_id: matching_category(&line.label, &rules),
                label: line.label,
                source: TransactionSource::Csv,
            })
            .collect();
        let total = incoming.len();
        let existing = self.transaction_provider.read_of_asset(&asset.id).await?;
        let new = service::transactions::new_transactions(&existing, incoming);
        // 4. Écriture : les transactions (toutes ou aucune), puis le solde.
        self.transaction_provider.write_many(&new).await?;
        // Le solde saisi l'emporte ; sinon celui que le relevé PDF imprime (il a été vérifié avec les opérations).
        let balance = match (command.closing_balance.or(pdf_balance), last_date) {
            (Some(amount), Some(valued_on)) => {
                let valuation = Valuation {
                    id: ValuationId::generate(),
                    asset_id: asset.id,
                    value: Money::new(amount, asset.currency),
                    valued_on,
                };
                self.valuation_provider.write(&valuation).await?;
                Some(valuation)
            }
            _ => None,
        };
        Ok(Output {
            imported: new.len(),
            already_known: total - new.len(),
            balance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asset::Asset;
    use category::CategoryRule;
    use institution::{CsvFormat, Institution};
    use shared::CategoryRuleId;
    use shared::Currency;
    use testing::{
        InMemoryAssets, InMemoryCategories, InMemoryInstitutions, InMemoryTransactions,
        InMemoryValuations,
    };

    const STATEMENT: &str = "Date;Libellé;Montant\n\
                             05/11/2026;Loyer;-700,00\n\
                             06/11/2026;Salaire;2 000,00\n";

    struct Setup {
        use_case: ImportTransactions,
        transactions: Arc<InMemoryTransactions>,
        valuations: Arc<InMemoryValuations>,
    }

    fn setup(kind: AssetKind) -> Setup {
        setup_with_rules(kind, [])
    }

    fn setup_with_rules(kind: AssetKind, rules: impl IntoIterator<Item = CategoryRule>) -> Setup {
        setup_full(kind, Some("Ma banque"), rules)
    }

    fn setup_full(
        kind: AssetKind,
        bank_name: Option<&str>,
        rules: impl IntoIterator<Item = CategoryRule>,
    ) -> Setup {
        let asset = Asset {
            id: "a".into(),
            name: "Compte courant".into(),
            kind,
            currency: Currency::Eur,
            bank_name: bank_name.map(Into::into),
            account_number: None,
        };
        let institutions = InMemoryInstitutions::with([Institution {
            name: "Ma banque".into(),
            csv: CsvFormat::suggested(),
            pdf: None,
        }]);
        let transactions = Arc::new(InMemoryTransactions::default());
        let valuations = Arc::new(InMemoryValuations::default());
        Setup {
            use_case: ImportTransactions {
                asset_provider: Arc::new(InMemoryAssets::with([asset])),
                transaction_provider: transactions.clone(),
                valuation_provider: valuations.clone(),
                category_provider: Arc::new(InMemoryCategories::with([], rules)),
                institution_provider: Arc::new(institutions),
                pdf_reader: Arc::new(testing::StubPdfReader::default()),
            },
            transactions,
            valuations,
        }
    }

    fn command(statement: &str, closing_balance: Option<Decimal>) -> Command {
        Command {
            asset_id: "a".into(),
            statement: statement.as_bytes().to_vec(),
            closing_balance,
            today: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
        }
    }

    #[tokio::test]
    async fn importe_les_operations_dans_la_devise_du_compte() {
        let s = setup(AssetKind::BankAccount);
        let output = s.use_case.handle(command(STATEMENT, None)).await.unwrap();

        assert_eq!((output.imported, output.already_known), (2, 0));
        assert!(output.balance.is_none());
        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].label, "Loyer");
        assert_eq!(
            stored[0].amount,
            Money::new(Decimal::from(-700), Currency::Eur)
        );
        assert!(s.valuations.read_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn classe_a_l_import_les_operations_que_les_regles_reconnaissent() {
        let rule = CategoryRule {
            id: CategoryRuleId::generate(),
            category_id: "logement".into(),
            pattern: "loyer".into(),
        };
        let s = setup_with_rules(AssetKind::BankAccount, [rule]);
        s.use_case.handle(command(STATEMENT, None)).await.unwrap();

        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored[0].category_id, Some("logement".into())); // « Loyer »
        assert_eq!(stored[1].category_id, None); // « Salaire »
    }

    #[tokio::test]
    async fn reimporter_le_meme_releve_ne_cree_pas_de_doublon() {
        let s = setup(AssetKind::BankAccount);
        s.use_case.handle(command(STATEMENT, None)).await.unwrap();
        let output = s.use_case.handle(command(STATEMENT, None)).await.unwrap();

        assert_eq!((output.imported, output.already_known), (0, 2));
        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[tokio::test]
    async fn le_solde_indique_devient_une_valorisation_a_la_date_de_la_derniere_operation() {
        let s = setup(AssetKind::BankAccount);
        let output = s
            .use_case
            .handle(command(STATEMENT, Some(Decimal::from(1_250))))
            .await
            .unwrap();

        let balance = output.balance.unwrap();
        assert_eq!(
            balance.value,
            Money::new(Decimal::from(1_250), Currency::Eur)
        );
        assert_eq!(
            balance.valued_on,
            NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()
        );
        assert_eq!(s.valuations.read_all().await.unwrap(), vec![balance]);
    }

    #[tokio::test]
    async fn refuse_un_actif_qui_n_est_pas_un_compte_bancaire() {
        let s = setup(AssetKind::SavingsBook);
        let result = s.use_case.handle(command(STATEMENT, None)).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_un_compte_sans_banque_en_disant_quoi_faire() {
        let s = setup_full(AssetKind::BankAccount, None, []);
        let Err(UseCaseError::Refusal(message)) = s.use_case.handle(command(STATEMENT, None)).await
        else {
            panic!("refus attendu");
        };
        assert!(message.contains("n'a pas de banque"), "{message}");
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_un_fichier_qui_ne_correspond_pas_au_format_de_la_banque() {
        let s = setup(AssetKind::BankAccount);
        // La banque attend une colonne « Libellé » : le fichier n'en a pas.
        let statement = "Date;Intitulé;Montant\n05/11/2026;Loyer;-700,00\n";
        let Err(UseCaseError::Refusal(message)) = s.use_case.handle(command(statement, None)).await
        else {
            panic!("refus attendu");
        };
        assert!(message.contains("« Libellé »"), "{message}");
    }

    fn word(text: &str, x0: f64, y: f64) -> institution::PositionedWord {
        institution::PositionedWord {
            text: text.into(),
            x0,
            x1: x0 + 5.0 * text.len() as f64,
            y,
        }
    }

    /// Un relevé minimal : un solde de 100 €, une dépense de 30 €, un solde final de 70 €.
    fn pdf_pages() -> Vec<Vec<institution::PositionedWord>> {
        vec![vec![
            word("du", 10.0, 700.0),
            word("01/11/2026", 25.0, 700.0),
            word("au", 80.0, 700.0),
            word("30/11/2026", 95.0, 700.0),
            word("Débit", 430.0, 600.0),
            word("Crédit", 520.0, 600.0),
            word("SOLDE", 74.0, 580.0),
            word("100,00", 520.0, 580.0),
            word("05.11", 30.0, 560.0),
            word("LOYER", 74.0, 560.0),
            word("30,00", 435.0, 560.0),
            word("TOTAL", 74.0, 540.0),
            word("SOLDE", 74.0, 520.0),
            word("70,00", 520.0, 520.0),
        ]]
    }

    fn setup_pdf(with_format: bool) -> Setup {
        let mut s = setup(AssetKind::BankAccount);
        let institutions = InMemoryInstitutions::with([Institution {
            name: "Ma banque".into(),
            csv: CsvFormat::suggested(),
            pdf: with_format.then(|| institution::PdfFormat {
                date_separator: institution::DaySeparator::Dot,
                end_marker: "TOTAL".into(),
                ..institution::PdfFormat::suggested()
            }),
        }]);
        s.use_case.institution_provider = Arc::new(institutions);
        s.use_case.pdf_reader = Arc::new(testing::StubPdfReader(Ok(pdf_pages())));
        s
    }

    #[tokio::test]
    async fn importe_un_releve_pdf_et_son_solde_final_devient_une_valorisation() {
        let s = setup_pdf(true);
        let mut pdf = command("%PDF-1.7", None);
        pdf.statement = b"%PDF-1.7 ...".to_vec();
        let output = s.use_case.handle(pdf).await.unwrap();

        assert_eq!(output.imported, 1);
        let stored = s.transactions.read_of_asset(&"a".into()).await.unwrap();
        assert_eq!(stored[0].label, "LOYER");
        assert_eq!(stored[0].amount.amount, Decimal::from(-30));
        // Le solde imprimé sur le relevé (vérifié avec les opérations) est enregistré.
        assert_eq!(output.balance.unwrap().value.amount, Decimal::from(70));
    }

    #[tokio::test]
    async fn refuse_un_pdf_si_la_banque_n_a_pas_de_format_pdf() {
        let s = setup_pdf(false);
        let mut pdf = command("", None);
        pdf.statement = b"%PDF-1.7 ...".to_vec();
        let Err(UseCaseError::Refusal(message)) = s.use_case.handle(pdf).await else {
            panic!("refus attendu");
        };
        assert!(message.contains("format de relevé PDF"), "{message}");
    }

    #[tokio::test]
    async fn refuse_un_fichier_illisible_sans_rien_enregistrer() {
        let s = setup(AssetKind::BankAccount);
        let result = s.use_case.handle(command("n'importe quoi", None)).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_une_operation_dans_le_futur() {
        let s = setup(AssetKind::BankAccount);
        let statement = "Date;Libellé;Montant\n11/11/2026;Demain;-1,00\n";
        let result = s.use_case.handle(command(statement, None)).await;
        assert!(matches!(result, Err(UseCaseError::Refusal(_))));
        assert!(
            s.transactions
                .read_of_asset(&"a".into())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn refuse_un_compte_inconnu() {
        let s = setup(AssetKind::BankAccount);
        let mut unknown = command(STATEMENT, None);
        unknown.asset_id = "inconnu".into();
        assert!(matches!(
            s.use_case.handle(unknown).await,
            Err(UseCaseError::Refusal(_))
        ));
    }
}
