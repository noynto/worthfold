//! Domaine de la connexion bancaire : le consentement donné à une banque, les comptes qu'elle expose,
//! et le port `BankAggregator` que l'agrégateur (EnableBanking) implémente.
//!
//! Le consentement dure au plus `CONSENT_DAYS` jours : passé ce délai, la banque refuse les lectures
//! et il faut le renouveler. Les opérations lues deviennent des transactions ordinaires (voir `transaction`).

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use shared::{AssetId, BankConnectionId, Currency};

/// Durée demandée pour un consentement (la directive européenne DSP2 l'a longtemps plafonné à 90 jours).
pub const CONSENT_DAYS: u64 = 90;
/// À partir de ce nombre de jours restants, la page invite à renouveler.
pub const RENEW_WARNING_DAYS: i64 = 14;

/// Un consentement accordé à une banque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankConnection {
    pub id: BankConnectionId,
    pub bank_name: String,
    /// Code pays ISO 3166 alpha-2 (« FR »).
    pub country: String,
    /// L'identifiant de session de l'agrégateur. Seul, il ne sert à rien : chaque appel est signé par
    /// la clé privée de l'application.
    pub session_id: String,
    /// Dernier jour où le consentement est valable.
    pub valid_until: NaiveDate,
    pub created_on: NaiveDate,
}

impl BankConnection {
    /// Jours restants (négatif une fois expiré).
    pub fn days_left(&self, today: NaiveDate) -> i64 {
        (self.valid_until - today).num_days()
    }
}

/// Un compte exposé par une banque, rattaché ou non à un compte bancaire de worthfold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankAccount {
    /// L'identifiant du compte chez l'agrégateur, valable pour la durée de la session.
    pub uid: String,
    pub connection_id: BankConnectionId,
    /// Ce qui reconnaît le compte d'une session à l'autre (renouvellement du consentement).
    pub identity: String,
    /// Nom ou IBAN partiellement masqué, à montrer.
    pub label: String,
    pub currency: Currency,
    /// Le compte de worthfold qui reçoit ses opérations ; absent tant que l'utilisateur n'a pas choisi.
    pub asset_id: Option<AssetId>,
    /// Jusqu'à quelle date les opérations ont été récupérées.
    pub last_synced_on: Option<NaiveDate>,
}

/// Une demande de consentement en cours : l'utilisateur est parti chez sa banque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAuthorization {
    /// Valeur aléatoire renvoyée par la banque au retour : protège contre un retour forgé.
    pub state: String,
    pub bank_name: String,
    pub country: String,
    pub created_on: NaiveDate,
}

#[async_trait]
pub trait BankConnectionProvider: Send + Sync {
    async fn read_connections(&self) -> anyhow::Result<Vec<BankConnection>>;
    async fn read_accounts(&self) -> anyhow::Result<Vec<BankAccount>>;
    /// Enregistre un consentement et ses comptes, tout ou rien.
    async fn write_connection(
        &self,
        connection: &BankConnection,
        accounts: &[BankAccount],
    ) -> anyhow::Result<()>;
    /// Supprime un consentement et ses comptes (les transactions déjà lues restent).
    async fn delete_connection(&self, id: &BankConnectionId) -> anyhow::Result<()>;
    /// Rattache (ou détache, avec `None`) un compte de la banque à un compte de worthfold.
    async fn link_account(&self, uid: &str, asset_id: Option<&AssetId>) -> anyhow::Result<()>;
    async fn set_synced(&self, uid: &str, date: NaiveDate) -> anyhow::Result<()>;
    /// Détache tous les comptes rattachés à cet actif (avant de le supprimer).
    async fn unlink_asset(&self, asset_id: &AssetId) -> anyhow::Result<()>;
    async fn write_pending(&self, pending: &PendingAuthorization) -> anyhow::Result<()>;
    /// Rend et retire la demande portant cet état : un état ne sert qu'une fois.
    async fn take_pending(&self, state: &str) -> anyhow::Result<Option<PendingAuthorization>>;
}

/// Une opération lue chez la banque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTransaction {
    pub booked_on: NaiveDate,
    /// Signé : négatif pour un débit.
    pub amount: Decimal,
    pub currency: Currency,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAccount {
    pub uid: String,
    pub identity: String,
    pub label: String,
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSession {
    pub session_id: String,
    pub valid_until: NaiveDate,
    pub accounts: Vec<RemoteAccount>,
}

/// Le port de l'agrégateur bancaire. Toute panne (réseau, banque, consentement expiré) est une erreur
/// technique, dont le message est journalisé et jamais montré tel quel.
#[async_trait]
pub trait BankAggregator: Send + Sync {
    /// Démarre un consentement : rend l'adresse de la banque où envoyer l'utilisateur.
    async fn authorize(
        &self,
        bank_name: &str,
        country: &str,
        valid_until: NaiveDate,
        state: &str,
    ) -> anyhow::Result<String>;
    /// Échange le code reçu au retour de la banque contre une session.
    async fn create_session(&self, code: &str) -> anyhow::Result<RemoteSession>;
    /// Ferme la session chez l'agrégateur (le consentement est retiré si la banque le permet).
    async fn close_session(&self, session_id: &str) -> anyhow::Result<()>;
    /// Les opérations comptabilisées entre deux dates (incluses), toutes pages réunies.
    async fn transactions(
        &self,
        account_uid: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<RemoteTransaction>>;
    /// Les opérations annoncées mais pas encore comptabilisées (paiements par carte récents).
    /// Une opération sans date prend celle de `today`.
    async fn pending_transactions(
        &self,
        account_uid: &str,
        today: NaiveDate,
    ) -> anyhow::Result<Vec<RemoteTransaction>>;
    /// Le solde comptabilisé du compte.
    async fn balance(&self, account_uid: &str) -> anyhow::Result<Option<Decimal>>;
    /// Les noms des banques que l'agrégateur sait connecter dans un pays, par ordre alphabétique.
    async fn institutions(&self, country: &str) -> anyhow::Result<Vec<String>>;
}
