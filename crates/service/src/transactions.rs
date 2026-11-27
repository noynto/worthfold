//! Règles sur les transactions d'un compte.

use std::collections::{HashMap, HashSet};

use category::{CategoryRule, matching_category};
use shared::{CategoryId, TransactionId};
use transaction::Transaction;

/// Ce qu'il faut réellement enregistrer parmi les transactions à importer : celles que le compte
/// ne connaît pas déjà. Réimporter un relevé qui recouvre le précédent ne crée donc aucun doublon.
///
/// Deux opérations identiques le même jour (deux cafés au même prix) restent deux opérations :
/// on compare les **nombres** d'occurrences, pas seulement la présence. Si le compte en connaît
/// déjà une et que le fichier en contient deux, une seule est nouvelle.
///
/// Deux temps. D'abord, l'empreinte exacte (date, montant, libellé). Ensuite, pour ce qui reste,
/// l'**autre source** : un même mouvement, vu par le relevé et par la banque connectée, n'a pas le même
/// libellé (la banque l'abrège et le met en capitales sans accents, le relevé y ajoute la contrepartie).
/// On le reconnaît à la même date et au même montant, quand tous les mots de l'un se retrouvent dans
/// l'autre (accents, casse et ponctuation ignorés). Les opérations d'une même source ne se rapprochent
/// jamais ainsi : deux prélèvements distincts du même jour restent deux opérations.
pub fn new_transactions(existing: &[Transaction], incoming: Vec<Transaction>) -> Vec<Transaction> {
    let mut used = vec![false; existing.len()];

    // Temps 1 : l'empreinte exacte.
    let mut by_fingerprint: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, transaction) in existing.iter().enumerate() {
        by_fingerprint
            .entry(transaction.fingerprint())
            .or_default()
            .push(i);
    }
    let mut remaining = Vec::new();
    for transaction in incoming {
        match by_fingerprint
            .get_mut(&transaction.fingerprint())
            .and_then(Vec::pop)
        {
            Some(i) => used[i] = true,
            None => remaining.push(transaction),
        }
    }

    // Temps 2 : le même mouvement vu par l'autre source.
    let mut by_day_and_amount: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, transaction) in existing.iter().enumerate().filter(|(i, _)| !used[*i]) {
        by_day_and_amount
            .entry(day_and_amount(transaction))
            .or_default()
            .push(i);
    }
    remaining
        .into_iter()
        .filter(|transaction| {
            let mine = words(&transaction.label);
            let twin = by_day_and_amount
                .get_mut(&day_and_amount(transaction))
                .and_then(|candidates| {
                    let position = candidates.iter().position(|&i| {
                        let known = &existing[i];
                        known.source != transaction.source
                            && labels_overlap(&words(&known.label), &mine)
                    })?;
                    Some(candidates.remove(position))
                });
            twin.is_none()
        })
        .collect()
}

fn day_and_amount(transaction: &Transaction) -> String {
    format!(
        "{}|{}|{}",
        transaction.booked_on,
        transaction.amount.amount.normalize(),
        transaction.amount.currency
    )
}

/// Les mots d'un libellé : sans accents, en minuscules, la ponctuation et les espaces séparant les mots.
fn words(label: &str) -> HashSet<String> {
    let folded: String = label
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect();
    folded.split_whitespace().map(str::to_owned).collect()
}

/// Tous les mots de l'un des libellés se retrouvent dans l'autre.
fn labels_overlap(a: &HashSet<String>, b: &HashSet<String>) -> bool {
    !a.is_empty() && !b.is_empty() && (a.is_subset(b) || b.is_subset(a))
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

    fn from_bank(label: &str, amount: i64) -> Transaction {
        Transaction {
            source: TransactionSource::Bank,
            ..transaction(label, amount)
        }
    }

    #[test]
    fn la_meme_operation_vue_par_la_banque_et_par_le_releve_n_est_comptee_qu_une_fois() {
        // Le relevé ajoute la contrepartie au libellé abrégé de la banque.
        let existing = vec![transaction(
            "Paiement par carte X2947 ANTHROPIC* CLAUDE SU 24/11 - Anthropic",
            -21,
        )];
        let incoming = vec![from_bank(
            "PAIEMENT PAR CARTE X2947 ANTHROPIC* CLAUDE SU 24/11",
            -21,
        )];
        assert!(new_transactions(&existing, incoming).is_empty());
        // Dans l'autre sens : la banque d'abord, le relevé ensuite.
        let existing = vec![from_bank("PAIEMENT PAR CARTE X2947 ANTHROPIC*", -21)];
        let incoming = vec![transaction(
            "Paiement par carte X2947 ANTHROPIC* - Anthropic",
            -21,
        )];
        assert!(new_transactions(&existing, incoming).is_empty());
    }

    #[test]
    fn deux_sources_ne_se_rapprochent_qu_a_la_meme_date_et_au_meme_montant() {
        let existing = vec![transaction("Paiement par carte ANTHROPIC - Anthropic", -21)];
        let other_amount = from_bank("Paiement par carte ANTHROPIC", -22);
        let other_label = from_bank("Virement SALAIRE", -21);
        let mut other_day = from_bank("Paiement par carte ANTHROPIC", -21);
        other_day.booked_on = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let new = new_transactions(&existing, vec![other_amount, other_label, other_day]);
        assert_eq!(new.len(), 3);
    }

    #[test]
    fn une_meme_source_ne_se_rapproche_jamais_par_prefixe() {
        // Deux prélèvements distincts du même jour, le second au libellé plus long.
        let existing = vec![transaction("Prélèvement IMPOT", -9)];
        let incoming = vec![transaction("Prélèvement IMPOT TF 25 REF 2", -9)];
        assert_eq!(new_transactions(&existing, incoming).len(), 1);
    }

    #[test]
    fn un_rapprochement_ne_sert_qu_une_fois() {
        // Un seul paiement connu : des deux lectures de la banque, une seule est le même mouvement.
        let existing = vec![transaction("Paiement par carte CAFE - Café", -3)];
        let incoming = vec![
            from_bank("Paiement par carte CAFE", -3),
            from_bank("Paiement par carte CAFE", -3),
        ];
        assert_eq!(new_transactions(&existing, incoming).len(), 1);
    }

    #[test]
    fn le_rapprochement_ignore_accents_ponctuation_et_mots_ajoutes() {
        let existing = vec![transaction(
            "Prélèvement ORANGE SA - Orange - Votre abonnement mobile: 06XXXXX991 (facture: 5336519526) EPCB202506302685710 ;FR18ZZZ002305 ;16543436795593139453365195265 EPCB202506302685710 FR18ZZZ002305 16543436795593139453365195265",
            -20,
        )];
        let incoming = vec![from_bank(
            "PRELEVEMENT ORANGE SA Votre abonnement mobile: 06XXXXX991 (facture: 5336519526) 16543436795593139453365195265 EPCB202506302685710 FR18ZZZ002305",
            -20,
        )];
        assert!(new_transactions(&existing, incoming).is_empty());
    }

    #[test]
    fn des_libelles_qui_n_ont_pas_tous_leurs_mots_en_commun_restent_distincts() {
        let existing = vec![transaction("Prélèvement ORANGE SA - Orange - mobile", -20)];
        let incoming = vec![from_bank("PRELEVEMENT EDF facture", -20)];
        assert_eq!(new_transactions(&existing, incoming).len(), 1);
    }
}
