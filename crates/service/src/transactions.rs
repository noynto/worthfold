//! Règles sur les transactions d'un compte.

use std::collections::HashMap;

use category::{CategoryRule, matching_category};
use shared::{CategoryId, TransactionId};
use transaction::Transaction;

/// Ce qu'il faut réellement enregistrer parmi les transactions à importer : celles que le compte
/// ne connaît pas déjà. Réimporter un relevé qui recouvre le précédent ne crée donc aucun doublon.
///
/// Deux opérations identiques le même jour (deux cafés au même prix) restent deux opérations :
/// on compare les **nombres** d'occurrences, pas seulement la présence. Si le compte en connaît
/// déjà une et que le fichier en contient deux, une seule est nouvelle.
pub fn new_transactions(existing: &[Transaction], incoming: Vec<Transaction>) -> Vec<Transaction> {
    let mut known: HashMap<String, usize> = HashMap::new();
    for transaction in existing {
        *known.entry(transaction.fingerprint()).or_default() += 1;
    }
    incoming
        .into_iter()
        .filter(
            |transaction| match known.get_mut(&transaction.fingerprint()) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    false
                }
                _ => true,
            },
        )
        .collect()
}

/// Les catégories que les règles donnent aux transactions **qui n'en ont pas encore**.
/// Une transaction déjà classée, à la main ou par une règle, n'est jamais reclassée : un choix de
/// l'utilisateur ne doit pas être écrasé par une règle ajoutée plus tard.
pub fn rule_assignments(
    transactions: &[Transaction],
    rules: &[CategoryRule],
) -> Vec<(TransactionId, CategoryId)> {
    transactions
        .iter()
        .filter(|t| t.category_id.is_none())
        .filter_map(|t| matching_category(&t.label, rules).map(|c| (t.id.clone(), c)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use shared::{Currency, Money, TransactionId};
    use transaction::TransactionSource;

    fn transaction(label: &str, amount: i64) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: "a".into(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            amount: Money::new(Decimal::from(amount), Currency::Eur),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: None,
        }
    }

    #[test]
    fn les_regles_ne_classent_que_les_transactions_sans_categorie() {
        let rule = CategoryRule {
            id: "r".into(),
            category_id: "logement".into(),
            pattern: "loyer".into(),
        };
        let free = transaction("PRLV LOYER", -700);
        let mut chosen = transaction("PRLV LOYER", -700);
        chosen.category_id = Some("autre".into());
        let unmatched = transaction("CB CAFE", -3);

        let assignments = rule_assignments(&[free.clone(), chosen, unmatched], &[rule]);
        assert_eq!(assignments, vec![(free.id, "logement".into())]);
    }

    #[test]
    fn tout_est_nouveau_sur_un_compte_vide() {
        let incoming = vec![transaction("Loyer", -700), transaction("Salaire", 2000)];
        assert_eq!(new_transactions(&[], incoming.clone()), incoming);
    }

    #[test]
    fn reimporter_le_meme_releve_n_ajoute_rien() {
        let existing = vec![transaction("Loyer", -700), transaction("Salaire", 2000)];
        let incoming = vec![transaction("Loyer", -700), transaction("Salaire", 2000)];
        assert!(new_transactions(&existing, incoming).is_empty());
    }

    #[test]
    fn un_releve_qui_recouvre_le_precedent_n_ajoute_que_le_nouveau() {
        let existing = vec![transaction("Loyer", -700)];
        let incoming = vec![transaction("Loyer", -700), transaction("Courses", -50)];
        let new = new_transactions(&existing, incoming);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].label, "Courses");
    }

    #[test]
    fn deux_operations_identiques_restent_deux_operations() {
        let incoming = vec![transaction("Café", -3), transaction("Café", -3)];
        assert_eq!(new_transactions(&[], incoming).len(), 2);
    }

    #[test]
    fn compare_les_nombres_d_occurrences() {
        let existing = vec![transaction("Café", -3)];
        let incoming = vec![transaction("Café", -3), transaction("Café", -3)];
        assert_eq!(new_transactions(&existing, incoming).len(), 1);
    }
}
