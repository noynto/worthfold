//! Domaine des transactions : les mouvements (débits et crédits) d'un compte bancaire.
//! Une transaction importée ne se modifie jamais, sauf sa catégorie : on ne la supprime pas non plus, sauf avec son compte.

use async_trait::async_trait;
use chrono::NaiveDate;
use shared::{AssetId, CategoryId, Money, TransactionId};

/// D'où vient la transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionSource {
    /// Importée depuis un export CSV de la banque.
    Csv,
    /// Lue chez la banque par l'agrégateur.
    Bank,
}

impl TransactionSource {
    /// Code stable, enregistré en base.
    pub fn code(self) -> &'static str {
        match self {
            TransactionSource::Csv => "csv",
            TransactionSource::Bank => "bank",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "csv" => Some(TransactionSource::Csv),
            "bank" => Some(TransactionSource::Bank),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: TransactionId,
    /// Le compte (un actif de type compte bancaire).
    pub asset_id: AssetId,
    /// La date d'opération.
    pub booked_on: NaiveDate,
    /// Signé : négatif pour une dépense, positif pour un revenu. Dans la devise du compte.
    pub amount: Money,
    /// Le libellé tel que la banque l'a donné, nettoyé de ses espaces superflus.
    pub label: String,
    pub source: TransactionSource,
    /// Le rangement (courses, loyer…), absent tant que ni une règle ni l'utilisateur ne l'a choisi.
    /// Seule partie d'une transaction qui change après l'import.
    pub category_id: Option<CategoryId>,
}

impl Transaction {
    /// Le libellé sans tenir compte de la casse ni des espaces superflus.
    pub fn normalized_label(&self) -> String {
        self.label
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// Ce qui identifie une opération d'un compte quand la banque ne donne pas d'identifiant :
    /// la date, le montant et le libellé (sans tenir compte de la casse ni des espaces).
    /// Sert à ne pas importer deux fois le même relevé (voir `service::new_transactions`).
    pub fn fingerprint(&self) -> String {
        let label = self.normalized_label();
        format!(
            "{}|{}|{}|{}",
            self.booked_on,
            self.amount.amount.normalize(),
            self.amount.currency,
            label
        )
    }
}

/// Une opération que la banque annonce mais n'a pas encore comptabilisée (paiement par carte récent).
/// Elle n'est pas une `Transaction` : elle peut changer de montant ou de libellé, ou disparaître, avant
/// d'être comptabilisée. Elle n'a ni identité ni catégorie, et chaque lecture de la banque remplace
/// toutes celles d'un compte (voir `TransactionProvider::replace_pending`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTransaction {
    pub asset_id: AssetId,
    /// La date annoncée par la banque.
    pub booked_on: NaiveDate,
    /// Signé, comme `Transaction::amount`.
    pub amount: Money,
    pub label: String,
}

#[async_trait]
pub trait TransactionProvider: Send + Sync {
    /// Toutes les transactions d'un compte, de la plus ancienne à la plus récente (à date égale, par ordre d'import).
    async fn read_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<Vec<Transaction>>;
    /// Toutes les transactions de tous les comptes, dans le même ordre.
    async fn read_all(&self) -> anyhow::Result<Vec<Transaction>>;
    async fn read(&self, id: &TransactionId) -> anyhow::Result<Option<Transaction>>;
    async fn write_many(&self, transactions: &[Transaction]) -> anyhow::Result<()>;
    /// Change (ou retire, avec `None`) la catégorie d'une transaction.
    async fn set_category(
        &self,
        id: &TransactionId,
        category_id: Option<&CategoryId>,
    ) -> anyhow::Result<()>;
    /// Affecte plusieurs catégories d'un coup : toutes ou aucune.
    async fn set_categories(
        &self,
        assignments: &[(TransactionId, CategoryId)],
    ) -> anyhow::Result<()>;
    /// Retire une catégorie de toutes les transactions qui la portent (avant de la supprimer).
    async fn clear_category(&self, category_id: &CategoryId) -> anyhow::Result<()>;
    /// Retire les transactions du compte, et ses opérations en attente.
    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()>;
    /// Remplace toutes les opérations en attente d'un compte par celles que la banque annonce maintenant
    /// (toutes ou aucune : une lecture à moitié écrite fausserait le solde disponible).
    async fn replace_pending(
        &self,
        asset_id: &AssetId,
        pending: &[PendingTransaction],
    ) -> anyhow::Result<()>;
    /// Les opérations en attente de tous les comptes, de la plus ancienne à la plus récente.
    async fn read_pending(&self) -> anyhow::Result<Vec<PendingTransaction>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use shared::Currency;

    fn transaction(label: &str, amount: Decimal) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            asset_id: "a".into(),
            booked_on: NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            amount: Money::new(amount, Currency::Eur),
            label: label.into(),
            source: TransactionSource::Csv,
            category_id: None,
        }
    }

    #[test]
    fn l_empreinte_ignore_la_casse_les_espaces_et_les_zeros_inutiles() {
        let a = transaction("CB  Carrefour   0412", Decimal::new(-1250, 2));
        let b = transaction("cb carrefour 0412", Decimal::new(-125, 1));
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn l_empreinte_distingue_le_montant_et_le_libelle() {
        let base = transaction("Loyer", Decimal::from(-700));
        assert_ne!(
            base.fingerprint(),
            transaction("Loyer", Decimal::from(-701)).fingerprint()
        );
        assert_ne!(
            base.fingerprint(),
            transaction("Électricité", Decimal::from(-700)).fingerprint()
        );
    }

    #[test]
    fn le_code_de_la_source_fait_l_aller_retour() {
        assert_eq!(
            TransactionSource::from_code(TransactionSource::Csv.code()),
            Some(TransactionSource::Csv)
        );
        assert_eq!(TransactionSource::from_code("inconnue"), None);
    }
}
