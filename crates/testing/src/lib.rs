//! Implémentations en mémoire des providers, pour les tests des cas d'usage.
//! Ajoutée seulement en `[dev-dependencies]` : jamais dans un binaire.

use std::sync::Mutex;

use asset::{Asset, AssetProvider};
use async_trait::async_trait;
use auth::{AuthProvider, Credential, PasswordHasher, Session};
use bank::{
    BankAccount, BankAggregator, BankConnection, BankConnectionProvider, PendingAuthorization,
    RemoteSession, RemoteTransaction,
};
use budget::{BudgetProvider, Envelope, SavingsPlan};
use category::{Category, CategoryProvider, CategoryRule};
use chrono::{DateTime, NaiveDate, Utc};
use institution::{Institution, InstitutionProvider, PdfReader, PositionedWord};
use recurrence::{ExceptionAction, PlannedPayment, RecurrenceProvider};
use shared::{
    AssetId, BankConnectionId, CategoryId, CategoryRuleId, PlannedPaymentId, TransactionId,
    ValuationId,
};
use transaction::{PendingTransaction, Transaction, TransactionProvider};
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

#[derive(Default)]
pub struct InMemoryTransactions(Mutex<Vec<Transaction>>, Mutex<Vec<PendingTransaction>>);

impl InMemoryTransactions {
    pub fn with(transactions: impl IntoIterator<Item = Transaction>) -> Self {
        Self(
            Mutex::new(transactions.into_iter().collect()),
            Mutex::default(),
        )
    }
}

#[async_trait]
impl TransactionProvider for InMemoryTransactions {
    async fn read_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<Vec<Transaction>> {
        let mut transactions: Vec<_> = self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|t| &t.asset_id == asset_id)
            .cloned()
            .collect();
        // Même ordre que la base : par date, puis par ordre d'import.
        transactions.sort_by(|a, b| (a.booked_on, &a.id).cmp(&(b.booked_on, &b.id)));
        Ok(transactions)
    }

    async fn read_all(&self) -> anyhow::Result<Vec<Transaction>> {
        let mut transactions = self.0.lock().unwrap().clone();
        transactions.sort_by(|a, b| (a.booked_on, &a.id).cmp(&(b.booked_on, &b.id)));
        Ok(transactions)
    }

    async fn read(&self, id: &TransactionId) -> anyhow::Result<Option<Transaction>> {
        Ok(self.0.lock().unwrap().iter().find(|t| &t.id == id).cloned())
    }

    async fn write_many(&self, transactions: &[Transaction]) -> anyhow::Result<()> {
        self.0.lock().unwrap().extend(transactions.iter().cloned());
        Ok(())
    }

    async fn set_category(
        &self,
        id: &TransactionId,
        category_id: Option<&CategoryId>,
    ) -> anyhow::Result<()> {
        if let Some(t) = self.0.lock().unwrap().iter_mut().find(|t| &t.id == id) {
            t.category_id = category_id.cloned();
        }
        Ok(())
    }

    async fn set_categories(
        &self,
        assignments: &[(TransactionId, CategoryId)],
    ) -> anyhow::Result<()> {
        let mut transactions = self.0.lock().unwrap();
        for (id, category_id) in assignments {
            if let Some(t) = transactions.iter_mut().find(|t| &t.id == id) {
                t.category_id = Some(category_id.clone());
            }
        }
        Ok(())
    }

    async fn clear_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        for t in self.0.lock().unwrap().iter_mut() {
            if t.category_id.as_ref() == Some(category_id) {
                t.category_id = None;
            }
        }
        Ok(())
    }

    async fn delete_of_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        self.0.lock().unwrap().retain(|t| &t.asset_id != asset_id);
        self.1.lock().unwrap().retain(|t| &t.asset_id != asset_id);
        Ok(())
    }

    async fn replace_pending(
        &self,
        asset_id: &AssetId,
        pending: &[PendingTransaction],
    ) -> anyhow::Result<()> {
        let mut all = self.1.lock().unwrap();
        all.retain(|t| &t.asset_id != asset_id);
        all.extend(pending.iter().cloned());
        Ok(())
    }

    async fn read_pending(&self) -> anyhow::Result<Vec<PendingTransaction>> {
        let mut pending = self.1.lock().unwrap().clone();
        pending.sort_by_key(|t| t.booked_on);
        Ok(pending)
    }
}

#[derive(Default)]
pub struct InMemoryCategories {
    categories: Mutex<Vec<Category>>,
    rules: Mutex<Vec<CategoryRule>>,
}

impl InMemoryCategories {
    pub fn with(
        categories: impl IntoIterator<Item = Category>,
        rules: impl IntoIterator<Item = CategoryRule>,
    ) -> Self {
        Self {
            categories: Mutex::new(categories.into_iter().collect()),
            rules: Mutex::new(rules.into_iter().collect()),
        }
    }
}

#[async_trait]
impl CategoryProvider for InMemoryCategories {
    async fn read_all(&self) -> anyhow::Result<Vec<Category>> {
        let mut categories = self.categories.lock().unwrap().clone();
        categories.sort_by_key(|c| c.name.to_lowercase());
        Ok(categories)
    }

    async fn read(&self, id: &CategoryId) -> anyhow::Result<Option<Category>> {
        Ok(self
            .categories
            .lock()
            .unwrap()
            .iter()
            .find(|c| &c.id == id)
            .cloned())
    }

    async fn write(&self, category: &Category) -> anyhow::Result<()> {
        self.categories.lock().unwrap().push(category.clone());
        Ok(())
    }

    async fn delete(&self, id: &CategoryId) -> anyhow::Result<()> {
        self.categories.lock().unwrap().retain(|c| &c.id != id);
        Ok(())
    }

    async fn read_rules(&self) -> anyhow::Result<Vec<CategoryRule>> {
        let mut rules = self.rules.lock().unwrap().clone();
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(rules)
    }

    async fn read_rule(&self, id: &CategoryRuleId) -> anyhow::Result<Option<CategoryRule>> {
        Ok(self
            .rules
            .lock()
            .unwrap()
            .iter()
            .find(|r| &r.id == id)
            .cloned())
    }

    async fn write_rule(&self, rule: &CategoryRule) -> anyhow::Result<()> {
        self.rules.lock().unwrap().push(rule.clone());
        Ok(())
    }

    async fn delete_rule(&self, id: &CategoryRuleId) -> anyhow::Result<()> {
        self.rules.lock().unwrap().retain(|r| &r.id != id);
        Ok(())
    }

    async fn delete_rules_of_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        self.rules
            .lock()
            .unwrap()
            .retain(|r| &r.category_id != category_id);
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryBudget {
    envelopes: Mutex<Vec<Envelope>>,
    plan: Mutex<Option<SavingsPlan>>,
}

impl InMemoryBudget {
    pub fn with(envelopes: impl IntoIterator<Item = Envelope>, plan: Option<SavingsPlan>) -> Self {
        Self {
            envelopes: Mutex::new(envelopes.into_iter().collect()),
            plan: Mutex::new(plan),
        }
    }
}

#[async_trait]
impl BudgetProvider for InMemoryBudget {
    async fn read_envelopes(&self) -> anyhow::Result<Vec<Envelope>> {
        let mut envelopes = self.envelopes.lock().unwrap().clone();
        envelopes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(envelopes)
    }

    async fn write_envelope(&self, envelope: &Envelope) -> anyhow::Result<()> {
        self.envelopes.lock().unwrap().push(envelope.clone());
        Ok(())
    }

    async fn read_savings_plan(&self) -> anyhow::Result<Option<SavingsPlan>> {
        Ok(self.plan.lock().unwrap().clone())
    }

    async fn write_savings_plan(&self, plan: &SavingsPlan) -> anyhow::Result<()> {
        *self.plan.lock().unwrap() = Some(plan.clone());
        Ok(())
    }

    async fn delete_envelopes_of_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        self.envelopes
            .lock()
            .unwrap()
            .retain(|e| &e.category_id != category_id);
        Ok(())
    }

    async fn delete_savings_plan(&self) -> anyhow::Result<()> {
        *self.plan.lock().unwrap() = None;
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryRecurrence {
    payments: Mutex<Vec<PlannedPayment>>,
    exceptions: Mutex<Vec<(PlannedPaymentId, NaiveDate, ExceptionAction)>>,
}

impl InMemoryRecurrence {
    pub fn with(payments: impl IntoIterator<Item = PlannedPayment>) -> Self {
        Self {
            payments: Mutex::new(payments.into_iter().collect()),
            exceptions: Mutex::default(),
        }
    }
}

#[async_trait]
impl RecurrenceProvider for InMemoryRecurrence {
    async fn read_all(&self) -> anyhow::Result<Vec<PlannedPayment>> {
        let mut payments = self.payments.lock().unwrap().clone();
        payments.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(payments)
    }

    async fn read(&self, id: &PlannedPaymentId) -> anyhow::Result<Option<PlannedPayment>> {
        Ok(self
            .payments
            .lock()
            .unwrap()
            .iter()
            .find(|p| &p.id == id)
            .cloned())
    }

    async fn write(&self, payment: &PlannedPayment) -> anyhow::Result<()> {
        let mut payments = self.payments.lock().unwrap();
        payments.retain(|p| p.id != payment.id);
        payments.push(payment.clone());
        Ok(())
    }

    async fn delete(&self, id: &PlannedPaymentId) -> anyhow::Result<()> {
        self.payments.lock().unwrap().retain(|p| &p.id != id);
        self.exceptions.lock().unwrap().retain(|(p, _, _)| p != id);
        Ok(())
    }

    async fn read_exceptions(
        &self,
    ) -> anyhow::Result<Vec<(PlannedPaymentId, NaiveDate, ExceptionAction)>> {
        Ok(self.exceptions.lock().unwrap().clone())
    }

    async fn write_exception(
        &self,
        payment_id: &PlannedPaymentId,
        date: NaiveDate,
        action: ExceptionAction,
    ) -> anyhow::Result<()> {
        let mut exceptions = self.exceptions.lock().unwrap();
        exceptions.retain(|(p, d, _)| !(p == payment_id && *d == date));
        exceptions.push((payment_id.clone(), date, action));
        Ok(())
    }

    async fn delete_exception(
        &self,
        payment_id: &PlannedPaymentId,
        date: NaiveDate,
    ) -> anyhow::Result<()> {
        self.exceptions
            .lock()
            .unwrap()
            .retain(|(p, d, _)| !(p == payment_id && *d == date));
        Ok(())
    }

    async fn clear_category(&self, category_id: &CategoryId) -> anyhow::Result<()> {
        for p in self.payments.lock().unwrap().iter_mut() {
            if p.category_id.as_ref() == Some(category_id) {
                p.category_id = None;
            }
        }
        Ok(())
    }

    async fn delete_of_account(&self, account_id: &AssetId) -> anyhow::Result<()> {
        let mut payments = self.payments.lock().unwrap();
        let removed: Vec<_> = payments
            .iter()
            .filter(|p| &p.account_id == account_id)
            .map(|p| p.id.clone())
            .collect();
        payments.retain(|p| &p.account_id != account_id);
        self.exceptions
            .lock()
            .unwrap()
            .retain(|(p, _, _)| !removed.contains(p));
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryBank {
    connections: Mutex<Vec<BankConnection>>,
    accounts: Mutex<Vec<BankAccount>>,
    pending: Mutex<Vec<PendingAuthorization>>,
}

impl InMemoryBank {
    pub fn with(connections: Vec<BankConnection>, accounts: Vec<BankAccount>) -> Self {
        Self {
            connections: Mutex::new(connections),
            accounts: Mutex::new(accounts),
            pending: Mutex::default(),
        }
    }
}

#[async_trait]
impl BankConnectionProvider for InMemoryBank {
    async fn read_connections(&self) -> anyhow::Result<Vec<BankConnection>> {
        Ok(self.connections.lock().unwrap().clone())
    }

    async fn read_accounts(&self) -> anyhow::Result<Vec<BankAccount>> {
        Ok(self.accounts.lock().unwrap().clone())
    }

    async fn write_connection(
        &self,
        connection: &BankConnection,
        accounts: &[BankAccount],
    ) -> anyhow::Result<()> {
        self.connections.lock().unwrap().push(connection.clone());
        self.accounts
            .lock()
            .unwrap()
            .extend(accounts.iter().cloned());
        Ok(())
    }

    async fn delete_connection(&self, id: &BankConnectionId) -> anyhow::Result<()> {
        self.connections.lock().unwrap().retain(|c| &c.id != id);
        self.accounts
            .lock()
            .unwrap()
            .retain(|a| &a.connection_id != id);
        Ok(())
    }

    async fn link_account(&self, uid: &str, asset_id: Option<&AssetId>) -> anyhow::Result<()> {
        for account in self
            .accounts
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|a| a.uid == uid)
        {
            account.asset_id = asset_id.cloned();
        }
        Ok(())
    }

    async fn set_synced(&self, uid: &str, date: NaiveDate) -> anyhow::Result<()> {
        for account in self
            .accounts
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|a| a.uid == uid)
        {
            account.last_synced_on = Some(date);
        }
        Ok(())
    }

    async fn unlink_asset(&self, asset_id: &AssetId) -> anyhow::Result<()> {
        for account in self.accounts.lock().unwrap().iter_mut() {
            if account.asset_id.as_ref() == Some(asset_id) {
                account.asset_id = None;
            }
        }
        Ok(())
    }

    async fn write_pending(&self, pending: &PendingAuthorization) -> anyhow::Result<()> {
        self.pending.lock().unwrap().push(pending.clone());
        Ok(())
    }

    async fn take_pending(&self, state: &str) -> anyhow::Result<Option<PendingAuthorization>> {
        let mut pending = self.pending.lock().unwrap();
        Ok(pending
            .iter()
            .position(|p| p.state == state)
            .map(|i| pending.remove(i)))
    }
}

/// Un agrégateur factice : rend ce qu'on lui a donné, et se souvient des appels.
#[derive(Default)]
pub struct FakeAggregator {
    pub session: Mutex<Option<RemoteSession>>,
    pub transactions: Mutex<Vec<RemoteTransaction>>,
    pub balance: Mutex<Option<rust_decimal::Decimal>>,
    /// Les (compte, début, fin) demandés, dans l'ordre.
    pub requests: Mutex<Vec<(String, NaiveDate, NaiveDate)>>,
    /// Les opérations en attente que la banque annonce.
    pub pending: Mutex<Vec<RemoteTransaction>>,
    pub institutions: Mutex<Vec<String>>,
    pub fail: Mutex<bool>,
    /// Seule la lecture des opérations en attente échoue.
    pub fail_pending: Mutex<bool>,
}

#[async_trait]
impl BankAggregator for FakeAggregator {
    async fn authorize(
        &self,
        bank_name: &str,
        country: &str,
        _valid_until: NaiveDate,
        state: &str,
    ) -> anyhow::Result<String> {
        Ok(format!(
            "https://bank.example/{country}/{bank_name}?state={state}"
        ))
    }

    async fn create_session(&self, _code: &str) -> anyhow::Result<RemoteSession> {
        self.session
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("pas de session"))
    }

    async fn transactions(
        &self,
        account_uid: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<RemoteTransaction>> {
        if *self.fail.lock().unwrap() {
            anyhow::bail!("panne simulée");
        }
        self.requests
            .lock()
            .unwrap()
            .push((account_uid.to_owned(), from, to));
        Ok(self.transactions.lock().unwrap().clone())
    }

    async fn pending_transactions(
        &self,
        _account_uid: &str,
        _today: NaiveDate,
    ) -> anyhow::Result<Vec<RemoteTransaction>> {
        if *self.fail_pending.lock().unwrap() {
            anyhow::bail!("panne simulée");
        }
        Ok(self.pending.lock().unwrap().clone())
    }

    async fn close_session(&self, _session_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn institutions(&self, _country: &str) -> anyhow::Result<Vec<String>> {
        if *self.fail.lock().unwrap() {
            anyhow::bail!("panne simulée");
        }
        Ok(self.institutions.lock().unwrap().clone())
    }

    async fn balance(&self, _account_uid: &str) -> anyhow::Result<Option<rust_decimal::Decimal>> {
        Ok(*self.balance.lock().unwrap())
    }
}

/// Le mot de passe et les sessions, en mémoire.
#[derive(Default)]
pub struct InMemoryAuth {
    credential: Mutex<Option<Credential>>,
    sessions: Mutex<Vec<Session>>,
}

impl InMemoryAuth {
    /// Un mot de passe (haché par [`PlainHasher`]) et des sessions déjà ouvertes.
    pub fn with(password: &str, sessions: impl IntoIterator<Item = Session>) -> Self {
        Self {
            credential: Mutex::new(Some(Credential {
                password_hash: PlainHasher.hash(password).unwrap(),
            })),
            sessions: Mutex::new(sessions.into_iter().collect()),
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }
}

#[async_trait]
impl AuthProvider for InMemoryAuth {
    async fn read_credential(&self) -> anyhow::Result<Option<Credential>> {
        Ok(self.credential.lock().unwrap().clone())
    }

    async fn write_credential(&self, credential: &Credential) -> anyhow::Result<()> {
        *self.credential.lock().unwrap() = Some(credential.clone());
        Ok(())
    }

    async fn write_session(&self, session: &Session) -> anyhow::Result<()> {
        self.sessions.lock().unwrap().push(session.clone());
        Ok(())
    }

    async fn read_session(&self, token_hash: &str) -> anyhow::Result<Option<Session>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.token_hash == token_hash)
            .cloned())
    }

    async fn delete_session(&self, token_hash: &str) -> anyhow::Result<()> {
        self.sessions
            .lock()
            .unwrap()
            .retain(|s| s.token_hash != token_hash);
        Ok(())
    }

    async fn delete_all_sessions(&self) -> anyhow::Result<()> {
        self.sessions.lock().unwrap().clear();
        Ok(())
    }

    async fn delete_expired_sessions(&self, now: DateTime<Utc>) -> anyhow::Result<()> {
        self.sessions.lock().unwrap().retain(|s| s.is_valid_at(now));
        Ok(())
    }

    async fn delete_credential(&self) -> anyhow::Result<()> {
        *self.credential.lock().unwrap() = None;
        self.sessions.lock().unwrap().clear();
        Ok(())
    }
}

/// Un « hachage » instantané pour les tests : l'empreinte est le mot de passe précédé d'un préfixe.
/// Jamais en production (voir `Argon2Hasher` dans l'infrastructure).
pub struct PlainHasher;

impl PasswordHasher for PlainHasher {
    fn hash(&self, password: &str) -> anyhow::Result<String> {
        Ok(format!("plain:{password}"))
    }

    fn verify(&self, password: &str, hash: &str) -> anyhow::Result<bool> {
        Ok(hash == format!("plain:{password}"))
    }
}

#[derive(Default)]
pub struct InMemoryInstitutions(Mutex<Vec<Institution>>);

impl InMemoryInstitutions {
    pub fn with(institutions: impl IntoIterator<Item = Institution>) -> Self {
        Self(Mutex::new(institutions.into_iter().collect()))
    }
}

#[async_trait]
impl InstitutionProvider for InMemoryInstitutions {
    async fn read_all(&self) -> anyhow::Result<Vec<Institution>> {
        let mut all = self.0.lock().unwrap().clone();
        all.sort_by_key(|i| i.name.to_lowercase());
        Ok(all)
    }

    async fn read(&self, name: &str) -> anyhow::Result<Option<Institution>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.name == name)
            .cloned())
    }

    async fn write(&self, institution: &Institution) -> anyhow::Result<()> {
        let mut all = self.0.lock().unwrap();
        all.retain(|i| i.name != institution.name);
        all.push(institution.clone());
        Ok(())
    }

    async fn delete(&self, name: &str) -> anyhow::Result<()> {
        self.0.lock().unwrap().retain(|i| i.name != name);
        Ok(())
    }
}

/// Un lecteur de PDF qui rend toujours les mêmes pages (ou le même refus).
pub struct StubPdfReader(pub Result<Vec<Vec<PositionedWord>>, String>);

impl Default for StubPdfReader {
    fn default() -> Self {
        Self(Err("Ce PDF est illisible : il est peut-être abîmé.".into()))
    }
}

impl PdfReader for StubPdfReader {
    fn read(&self, _bytes: &[u8]) -> Result<Vec<Vec<PositionedWord>>, String> {
        self.0.clone()
    }
}
