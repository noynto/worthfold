//! L'adaptateur EnableBanking du port `BankAggregator` (https://enablebanking.com/docs/api/reference/).
//!
//! Chaque appel porte un JWT signé en RS256 par la clé privée de l'application : l'identifiant de
//! l'application est le `kid`, la durée de vie du jeton est courte (5 minutes, le maximum accepté est 24 h).
//! La clé ne quitte jamais ce module et n'est jamais journalisée.

use std::path::Path;

use anyhow::{Context, bail};
use async_trait::async_trait;
use bank::{BankAggregator, RemoteAccount, RemoteSession, RemoteTransaction};
use chrono::{NaiveDate, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::{Client, RequestBuilder};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared::Currency;

pub const BASE_URL: &str = "https://api.enablebanking.com";
const TOKEN_TTL_SECONDS: i64 = 300;
/// Combien de jours en arrière chercher des opérations en attente.
const PENDING_LOOKBACK_DAYS: u64 = 30;

pub struct EnableBanking {
    http: Client,
    base_url: String,
    application_id: String,
    key: EncodingKey,
    redirect_url: String,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

impl EnableBanking {
    /// Lit la clé privée (PEM) et prépare le client. Une clé illisible est refusée dès le démarrage.
    pub fn from_pem_file(
        application_id: String,
        key_path: &Path,
        redirect_url: String,
    ) -> anyhow::Result<Self> {
        let pem = std::fs::read(key_path)
            .with_context(|| format!("Impossible de lire la clé privée {}.", key_path.display()))?;
        Self::new(BASE_URL.to_owned(), application_id, &pem, redirect_url)
    }

    pub fn new(
        base_url: String,
        application_id: String,
        pem: &[u8],
        redirect_url: String,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            base_url,
            application_id,
            key: EncodingKey::from_rsa_pem(pem)
                .context("La clé privée n'est pas une clé RSA au format PEM.")?,
            redirect_url,
        })
    }

    /// Le jeton de l'appel : `iss`, `aud`, `iat`, `exp`, et le `kid` dans l'en-tête.
    fn token(&self) -> anyhow::Result<String> {
        let now = Utc::now().timestamp();
        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some("JWT".into());
        header.kid = Some(self.application_id.clone());
        encode(
            &header,
            &Claims {
                iss: "enablebanking.com",
                aud: "api.enablebanking.com",
                // Une minute de marge si l'horloge locale avance un peu sur celle du serveur.
                iat: now - 60,
                exp: now + TOKEN_TTL_SECONDS,
            },
            &self.key,
        )
        .context("Impossible de signer le jeton.")
    }

    /// Lit toutes les pages d'opérations d'un statut (`BOOK` comptabilisées, `PDNG` en attente).
    /// `default_day` sert aux opérations sans aucune date (fréquent pour celles en attente).
    async fn fetch_transactions(
        &self,
        account_uid: &str,
        from: NaiveDate,
        to: NaiveDate,
        status: &str,
        default_day: Option<NaiveDate>,
    ) -> anyhow::Result<Vec<RemoteTransaction>> {
        let mut found = Vec::new();
        let mut continuation: Option<String> = None;
        // Une page peut être vide et porter pourtant une clé de suite : on suit la clé jusqu'au bout,
        // avec une borne pour ne jamais tourner indéfiniment.
        for _ in 0..200 {
            let mut request = self
                .get(&format!("/accounts/{account_uid}/transactions"))?
                .query(&[
                    ("date_from", from.to_string()),
                    ("date_to", to.to_string()),
                    ("transaction_status", status.to_owned()),
                ]);
            if let Some(key) = &continuation {
                request = request.query(&[("continuation_key", key)]);
            }
            let page: TransactionsResponse = send(request).await?;
            for t in &page.transactions {
                let day = match t
                    .booking_date
                    .as_deref()
                    .or(t.value_date.as_deref())
                    .or(t.transaction_date.as_deref())
                {
                    Some(day) => parse_day(day)?,
                    None => match default_day {
                        Some(day) => day,
                        None => continue,
                    },
                };
                let amount: Decimal = t
                    .transaction_amount
                    .amount
                    .parse()
                    .context("Montant illisible chez la banque.")?;
                let amount = amount.abs();
                found.push(RemoteTransaction {
                    booked_on: day,
                    amount: if t.credit_debit_indicator == "DBIT" {
                        -amount
                    } else {
                        amount
                    },
                    currency: t
                        .transaction_amount
                        .currency
                        .parse::<Currency>()
                        .map_err(|_| anyhow::anyhow!("Devise non prise en charge."))?,
                    label: label(t),
                });
            }
            continuation = page.continuation_key.filter(|k| !k.is_empty());
            if continuation.is_none() {
                return Ok(found);
            }
        }
        bail!("Trop de pages d'opérations : lecture interrompue.")
    }

    fn get(&self, path: &str) -> anyhow::Result<RequestBuilder> {
        Ok(self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(self.token()?))
    }

    fn post(&self, path: &str) -> anyhow::Result<RequestBuilder> {
        Ok(self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(self.token()?))
    }
}

/// Envoie la requête et rend le JSON, ou une erreur qui cite le code et le message de l'API.
async fn send<T: for<'de> Deserialize<'de>>(request: RequestBuilder) -> anyhow::Result<T> {
    let response = request
        .send()
        .await
        .map_err(reqwest::Error::without_url)
        .context("EnableBanking injoignable.")?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("EnableBanking a répondu {status} : {body}");
    }
    serde_json::from_str(&body).context("Réponse d'EnableBanking illisible.")
}

#[derive(Deserialize)]
struct InstitutionsResponse {
    aspsps: Vec<ApiInstitution>,
}

#[derive(Deserialize)]
struct ApiInstitution {
    name: String,
}

#[derive(Deserialize)]
struct AuthResponse {
    url: String,
}

#[derive(Deserialize)]
struct SessionResponse {
    session_id: String,
    accounts: Vec<ApiAccount>,
    access: ApiAccess,
}

#[derive(Deserialize)]
struct ApiAccess {
    valid_until: String,
}

#[derive(Deserialize)]
struct ApiAccount {
    uid: String,
    #[serde(default)]
    account_id: Option<ApiAccountId>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    details: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    identification_hash: Option<String>,
}

#[derive(Deserialize)]
struct ApiAccountId {
    #[serde(default)]
    iban: Option<String>,
}

#[derive(Deserialize)]
struct TransactionsResponse {
    #[serde(default)]
    transactions: Vec<ApiTransaction>,
    #[serde(default)]
    continuation_key: Option<String>,
}

#[derive(Deserialize)]
struct ApiTransaction {
    transaction_amount: ApiAmount,
    credit_debit_indicator: String,
    #[serde(default)]
    booking_date: Option<String>,
    #[serde(default)]
    value_date: Option<String>,
    #[serde(default)]
    transaction_date: Option<String>,
    #[serde(default)]
    remittance_information: Vec<String>,
    #[serde(default)]
    creditor: Option<ApiParty>,
    #[serde(default)]
    debtor: Option<ApiParty>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct ApiParty {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ApiAmount {
    currency: String,
    amount: String,
}

#[derive(Deserialize)]
struct BalancesResponse {
    #[serde(default)]
    balances: Vec<ApiBalance>,
}

#[derive(Deserialize)]
struct ApiBalance {
    balance_amount: ApiAmount,
    #[serde(default)]
    balance_type: String,
}

/// « 2026-12-01T12:00:00+00:00 » → le jour. Le consentement vaut jusqu'à cette date incluse.
fn parse_day(text: &str) -> anyhow::Result<NaiveDate> {
    let day = text.get(..10).unwrap_or(text);
    NaiveDate::parse_from_str(day, "%Y-%m-%d").with_context(|| format!("Date illisible : {text}"))
}

/// Un IBAN dont on ne montre que le début et la fin : « FR76 … 4321 ».
fn masked(iban: &str) -> String {
    let compact: String = iban.split_whitespace().collect();
    if compact.chars().count() <= 8 {
        return compact;
    }
    let head: String = compact.chars().take(4).collect();
    let tail: String = compact
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head} … {tail}")
}

fn account(api: ApiAccount) -> anyhow::Result<RemoteAccount> {
    let iban = api.account_id.and_then(|a| a.iban);
    // Ce qui reconnaît le compte d'une session à l'autre : le hash d'identification, sinon l'IBAN.
    let identity = api
        .identification_hash
        .or_else(|| iban.clone())
        .unwrap_or_else(|| api.uid.clone());
    let label = [api.name, api.details]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .or_else(|| iban.as_deref().map(masked))
        .unwrap_or_else(|| "Compte".to_owned());
    let label = match (&iban, label.as_str()) {
        (Some(iban), name) if !name.contains('…') => format!("{name} ({})", masked(iban)),
        _ => label,
    };
    Ok(RemoteAccount {
        uid: api.uid,
        identity,
        label,
        currency: api
            .currency
            .as_deref()
            .unwrap_or("EUR")
            .parse()
            .map_err(|_| anyhow::anyhow!("Devise non prise en charge."))?,
    })
}

/// Le libellé d'une opération : le motif de virement, sinon la contrepartie, sinon la note.
fn label(t: &ApiTransaction) -> String {
    let remittance = t.remittance_information.join(" ");
    let counterparty = if t.credit_debit_indicator == "DBIT" {
        t.creditor.as_ref().and_then(|p| p.name.clone())
    } else {
        t.debtor.as_ref().and_then(|p| p.name.clone())
    };
    [Some(remittance), counterparty, t.note.clone()]
        .into_iter()
        .flatten()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "Opération sans libellé".to_owned())
}

#[async_trait]
impl BankAggregator for EnableBanking {
    async fn authorize(
        &self,
        bank_name: &str,
        country: &str,
        valid_until: NaiveDate,
        state: &str,
    ) -> anyhow::Result<String> {
        let valid_until = valid_until
            .and_hms_opt(12, 0, 0)
            .context("date invalide")?
            .and_utc()
            .to_rfc3339();
        let body = json!({
            "access": { "valid_until": valid_until },
            "aspsp": { "name": bank_name, "country": country },
            "state": state,
            "redirect_url": self.redirect_url,
            "psu_type": "personal",
        });
        let response: AuthResponse = send(self.post("/auth")?.json(&body)).await?;
        Ok(response.url)
    }

    async fn create_session(&self, code: &str) -> anyhow::Result<RemoteSession> {
        let response: SessionResponse =
            send(self.post("/sessions")?.json(&json!({ "code": code }))).await?;
        Ok(RemoteSession {
            session_id: response.session_id,
            valid_until: parse_day(&response.access.valid_until)?,
            accounts: response
                .accounts
                .into_iter()
                .map(account)
                .collect::<anyhow::Result<_>>()?,
        })
    }

    async fn transactions(
        &self,
        account_uid: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<RemoteTransaction>> {
        self.fetch_transactions(account_uid, from, to, "BOOK", None)
            .await
    }

    async fn pending_transactions(
        &self,
        account_uid: &str,
        today: NaiveDate,
    ) -> anyhow::Result<Vec<RemoteTransaction>> {
        // Une opération en attente a au plus quelques jours ; le mois écoulé suffit largement.
        self.fetch_transactions(
            account_uid,
            today - chrono::Days::new(PENDING_LOOKBACK_DAYS),
            today,
            "PDNG",
            Some(today),
        )
        .await
    }

    async fn close_session(&self, session_id: &str) -> anyhow::Result<()> {
        let response = self
            .http
            .delete(format!("{}/sessions/{session_id}", self.base_url))
            .bearer_auth(self.token()?)
            .send()
            // L'adresse de la fermeture contient le `session_id` : elle ne doit pas finir dans les journaux.
            .await
            .map_err(reqwest::Error::without_url)
            .context("EnableBanking injoignable.")?;
        if !response.status().is_success() {
            bail!(
                "EnableBanking a répondu {} à la fermeture.",
                response.status()
            );
        }
        Ok(())
    }

    async fn institutions(&self, country: &str) -> anyhow::Result<Vec<String>> {
        let response: InstitutionsResponse = send(
            self.get("/aspsps")?
                .query(&[("country", country.to_uppercase())]),
        )
        .await?;
        let mut names: Vec<String> = response.aspsps.into_iter().map(|a| a.name).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        Ok(names)
    }

    async fn balance(&self, account_uid: &str) -> anyhow::Result<Option<Decimal>> {
        let response: BalancesResponse =
            send(self.get(&format!("/accounts/{account_uid}/balances"))?).await?;
        // CLBD : solde comptabilisé de clôture ; à défaut, le premier solde disponible.
        let balance = response
            .balances
            .iter()
            .find(|b| b.balance_type == "CLBD")
            .or(response.balances.first());
        balance
            .map(|b| {
                let amount: Decimal = b
                    .balance_amount
                    .amount
                    .parse()
                    .context("Solde illisible chez la banque.")?;
                Ok(amount)
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masque_un_iban() {
        assert_eq!(masked("FR76 3000 6000 0112 3456 7890 189"), "FR76 … 0189");
        assert_eq!(masked("FR76"), "FR76");
    }

    #[test]
    fn lit_le_jour_d_une_date_avec_heure() {
        let day = parse_day("2026-12-01T12:00:00.000000+00:00").unwrap();
        assert_eq!(day, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
    }

    #[test]
    fn le_libelle_prefere_le_motif_puis_la_contrepartie() {
        let base = |remittance: Vec<&str>, creditor: Option<&str>| ApiTransaction {
            transaction_amount: ApiAmount {
                currency: "EUR".into(),
                amount: "1".into(),
            },
            credit_debit_indicator: "DBIT".into(),
            booking_date: None,
            value_date: None,
            transaction_date: None,
            remittance_information: remittance.into_iter().map(str::to_owned).collect(),
            creditor: creditor.map(|n| ApiParty {
                name: Some(n.into()),
            }),
            debtor: None,
            note: None,
        };
        assert_eq!(
            label(&base(vec!["PRLV  EDF", "FACTURE"], Some("EDF SA"))),
            "PRLV EDF FACTURE"
        );
        assert_eq!(label(&base(vec![], Some("EDF SA"))), "EDF SA");
        assert_eq!(label(&base(vec![], None)), "Opération sans libellé");
    }

    #[test]
    fn refuse_une_cle_qui_n_est_pas_du_pem() {
        let error = EnableBanking::new(
            BASE_URL.into(),
            "id".into(),
            b"pas une cle",
            "http://x".into(),
        );
        assert!(error.is_err());
    }

    /// Une paire de clés jetable, générée avec openssl (le test est ignoré sans lui).
    fn key_pair() -> Option<(Vec<u8>, Vec<u8>)> {
        let dir =
            std::env::temp_dir().join(format!("wf-key-{}", shared::TransactionId::generate()));
        std::fs::create_dir_all(&dir).ok()?;
        let private = dir.join("k.pem");
        let ok = std::process::Command::new("openssl")
            .args([
                "genpkey",
                "-algorithm",
                "RSA",
                "-pkeyopt",
                "rsa_keygen_bits:2048",
                "-out",
            ])
            .arg(&private)
            .output()
            .ok()?
            .status
            .success();
        let public = std::process::Command::new("openssl")
            .args(["pkey", "-pubout", "-in"])
            .arg(&private)
            .output()
            .ok()?;
        let result = (ok && public.status.success())
            .then(|| (std::fs::read(&private).unwrap(), public.stdout));
        let _ = std::fs::remove_dir_all(dir);
        result
    }

    #[tokio::test]
    async fn une_erreur_reseau_ne_laisse_pas_le_session_id_dans_le_message() {
        let Some((private, _)) = key_pair() else {
            return;
        };
        // Un port où rien n'écoute : la fermeture échoue avant toute réponse.
        let client = EnableBanking::new(
            "http://127.0.0.1:1".into(),
            "app-123".into(),
            &private,
            "http://localhost/cb".into(),
        )
        .unwrap();
        let error = client.close_session("SECRET-SESSION-ID").await.unwrap_err();
        assert!(!format!("{error:?}").contains("SECRET-SESSION-ID"));
    }

    #[tokio::test]
    async fn signe_chaque_appel_et_suit_les_pages_d_operations() {
        use axum::extract::Query;
        use axum::http::HeaderMap;
        use axum::routing::get;
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let Some((private, public)) = key_pair() else {
            return;
        };
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = seen.clone();
        let app = axum::Router::new().route(
            "/accounts/uid-1/transactions",
            get(
                move |headers: HeaderMap, Query(q): Query<HashMap<String, String>>| {
                    let log = log.clone();
                    async move {
                        log.lock()
                            .unwrap()
                            .push(headers["authorization"].to_str().unwrap().to_owned());
                        let page = match q.get("continuation_key").map(String::as_str) {
                            None => serde_json::json!({
                                "transactions": [{
                                    "transaction_amount": {"currency": "EUR", "amount": "12.50"},
                                    "credit_debit_indicator": "DBIT",
                                    "booking_date": "2026-11-05",
                                    "remittance_information": ["PRLV NETFLIX"]
                                }],
                                "continuation_key": "page-2"
                            }),
                            Some(_) => serde_json::json!({
                                "transactions": [{
                                    "transaction_amount": {"currency": "EUR", "amount": "2000.00"},
                                    "credit_debit_indicator": "CRDT",
                                    "booking_date": "2026-11-06",
                                    "debtor": {"name": "EMPLOYEUR"}
                                }],
                                "continuation_key": null
                            }),
                        };
                        assert_eq!(q["date_from"], "2026-11-01");
                        axum::Json(page)
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let client = EnableBanking::new(
            base,
            "app-123".into(),
            &private,
            "http://localhost/cb".into(),
        )
        .unwrap();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
        let found = client.transactions("uid-1", day(1), day(30)).await.unwrap();

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].amount, Decimal::new(-1250, 2));
        assert_eq!(found[0].label, "PRLV NETFLIX");
        assert_eq!(found[1].amount, Decimal::from(2000));
        assert_eq!(found[1].label, "EMPLOYEUR");

        // Le jeton est un vrai RS256, vérifiable avec la clé publique, au bon `kid` et aux bonnes revendications.
        let token = seen.lock().unwrap()[0]
            .strip_prefix("Bearer ")
            .unwrap()
            .to_owned();
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some("app-123"));
        let mut validation = jsonwebtoken::Validation::new(Algorithm::RS256);
        validation.set_audience(&["api.enablebanking.com"]);
        validation.set_issuer(&["enablebanking.com"]);
        let data = jsonwebtoken::decode::<serde_json::Value>(
            &token,
            &jsonwebtoken::DecodingKey::from_rsa_pem(&public).unwrap(),
            &validation,
        )
        .unwrap();
        let ttl = data.claims["exp"].as_i64().unwrap() - data.claims["iat"].as_i64().unwrap();
        assert!(ttl <= 86_400, "{ttl}");
    }
}
