use std::sync::Arc;

use bank::BankAggregator;

pub struct ListBanks {
    /// Absent tant que la connexion bancaire n'est pas configurée.
    pub aggregator: Option<Arc<dyn BankAggregator>>,
}

pub struct Query {
    /// Code pays à deux lettres, tel qu'il se tape : il peut être incomplet.
    pub country: String,
}

pub struct Output {
    /// Les banques connectables dans ce pays, par ordre alphabétique ; vide si le pays est incomplet.
    pub banks: Vec<String>,
}

impl ListBanks {
    /// Des suggestions pour le champ « Banque » : leur absence ne bloque rien, l'utilisateur peut taper le nom.
    /// Une panne de l'agrégateur donne donc une liste vide (journalisée), pas une erreur.
    pub async fn handle(&self, query: Query) -> Output {
        let country = query.country.trim().to_uppercase();
        let valid = country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic());
        let (Some(aggregator), true) = (&self.aggregator, valid) else {
            return Output { banks: Vec::new() };
        };
        match aggregator.institutions(&country).await {
            Ok(banks) => Output { banks },
            Err(e) => {
                tracing::warn!(error = ?e, "Liste des banques indisponible.");
                Output { banks: Vec::new() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testing::FakeAggregator;

    fn use_case(fake: &Arc<FakeAggregator>) -> ListBanks {
        ListBanks {
            aggregator: Some(fake.clone()),
        }
    }

    #[tokio::test]
    async fn propose_les_banques_du_pays() {
        let fake = Arc::new(FakeAggregator::default());
        *fake.institutions.lock().unwrap() = vec!["BBVA".into(), "Genome".into()];
        let output = use_case(&fake)
            .handle(Query {
                country: " fr ".into(),
            })
            .await;
        assert_eq!(output.banks, ["BBVA", "Genome"]);
    }

    #[tokio::test]
    async fn un_pays_incomplet_ou_une_panne_donnent_une_liste_vide() {
        let fake = Arc::new(FakeAggregator::default());
        *fake.institutions.lock().unwrap() = vec!["BBVA".into()];
        assert!(
            use_case(&fake)
                .handle(Query {
                    country: "F".into()
                })
                .await
                .banks
                .is_empty()
        );
        *fake.fail.lock().unwrap() = true;
        assert!(
            use_case(&fake)
                .handle(Query {
                    country: "FR".into()
                })
                .await
                .banks
                .is_empty()
        );
        let sans_banque = ListBanks { aggregator: None };
        assert!(
            sans_banque
                .handle(Query {
                    country: "FR".into()
                })
                .await
                .banks
                .is_empty()
        );
    }
}
