//! Domaine de l'authentification : un seul utilisateur, un mot de passe, des sessions.
//!
//! Le mot de passe n'est jamais conservé : seule son empreinte (calculée par un `PasswordHasher`, lent à
//! dessein) l'est. Un jeton de session n'est jamais conservé non plus : la base ne garde que son empreinte,
//! si bien qu'une copie de la base ne permet pas de se faire passer pour l'utilisateur.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

/// Durée d'une session : au-delà, il faut saisir de nouveau le mot de passe.
pub const SESSION_DAYS: i64 = 14;
/// Longueur minimale d'un mot de passe : une phrase de passe courte vaut mieux qu'un mot compliqué.
pub const MIN_PASSWORD_LENGTH: usize = 12;
/// Longueur maximale : borne le travail de l'empreinte (un mot de passe d'un mégaoctet épuiserait le serveur).
pub const MAX_PASSWORD_LENGTH: usize = 256;

/// L'empreinte du mot de passe de l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub password_hash: String,
}

/// Une session ouverte : l'empreinte de son jeton et ses dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// SHA-256 du jeton, en hexadécimal (voir [`hash_token`]).
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Session {
    /// Une session ouverte à `now`, valable [`SESSION_DAYS`] jours.
    pub fn open(token: &str, now: DateTime<Utc>) -> Self {
        Self {
            token_hash: hash_token(token),
            created_at: now,
            expires_at: now + Duration::days(SESSION_DAYS),
        }
    }

    pub fn is_valid_at(&self, now: DateTime<Utc>) -> bool {
        now < self.expires_at
    }
}

/// L'empreinte d'un jeton de session. Le jeton est aléatoire (plus de 200 bits) : un hachage rapide suffit,
/// il n'y a rien à deviner par force brute.
pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Ce qu'un mot de passe doit respecter pour être accepté, en messages pour l'utilisateur.
pub fn check_new_password(password: &str, confirmation: &str) -> Result<(), String> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_LENGTH {
        return Err(format!(
            "Le mot de passe doit faire au moins {MIN_PASSWORD_LENGTH} caractères : une phrase de passe convient très bien."
        ));
    }
    if length > MAX_PASSWORD_LENGTH {
        return Err(format!(
            "Le mot de passe ne peut pas dépasser {MAX_PASSWORD_LENGTH} caractères."
        ));
    }
    if password != confirmation {
        return Err("Les deux mots de passe ne sont pas identiques.".to_owned());
    }
    Ok(())
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Le mot de passe de l'utilisateur, absent tant que l'application n'est pas configurée.
    async fn read_credential(&self) -> anyhow::Result<Option<Credential>>;
    /// Enregistre le mot de passe, en remplaçant le précédent.
    async fn write_credential(&self, credential: &Credential) -> anyhow::Result<()>;
    async fn write_session(&self, session: &Session) -> anyhow::Result<()>;
    async fn read_session(&self, token_hash: &str) -> anyhow::Result<Option<Session>>;
    async fn delete_session(&self, token_hash: &str) -> anyhow::Result<()>;
    /// Ferme toutes les sessions (après un changement de mot de passe).
    async fn delete_all_sessions(&self) -> anyhow::Result<()>;
    /// Retire les sessions expirées à `now`.
    async fn delete_expired_sessions(&self, now: DateTime<Utc>) -> anyhow::Result<()>;
    /// Retire le mot de passe et toutes les sessions : l'application redemande une configuration.
    async fn delete_credential(&self) -> anyhow::Result<()>;
}

/// Calcule et vérifie les empreintes de mot de passe. Un échec technique est une erreur ; un mot de passe
/// qui ne correspond pas est `Ok(false)`.
pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &str) -> anyhow::Result<String>;
    fn verify(&self, password: &str, hash: &str) -> anyhow::Result<bool>;
}

/// Freine les essais de mot de passe, **par client** (son adresse IP, voir `server`) : après
/// [`Throttle::FREE_ATTEMPTS`] échecs de suite, chaque nouvel échec ferme la porte plus longtemps pour ce client
/// (une minute, puis le double, jusqu'à un quart d'heure). Un tiers qui s'acharne ne bloque donc pas le
/// propriétaire, qui vient d'une autre adresse.
///
/// Un client qui n'a pas échoué depuis [`Throttle::FORGET_AFTER_MINUTES`] est oublié (plus que la plus longue
/// attente). La table est bornée à [`Throttle::MAX_CLIENTS`] : au-delà, le client le plus ancien est retiré, pour
/// qu'une multitude d'adresses ne fasse pas grossir la mémoire. Elle vit en mémoire et repart à zéro au redémarrage.
#[derive(Debug, Default)]
pub struct Throttle {
    clients: Mutex<HashMap<String, ClientState>>,
}

#[derive(Debug)]
struct ClientState {
    failures: u32,
    last_failure: DateTime<Utc>,
    locked_until: Option<DateTime<Utc>>,
}

impl Throttle {
    pub const FREE_ATTEMPTS: u32 = 5;
    pub const MAX_CLIENTS: usize = 10_000;
    pub const FORGET_AFTER_MINUTES: i64 = 60;
    const BASE_LOCK_SECONDS: i64 = 60;
    const MAX_LOCK_SECONDS: i64 = 15 * 60;

    /// `Err(secondes)` tant que la porte est fermée pour ce client.
    pub fn check(&self, client: &str, now: DateTime<Utc>) -> Result<(), i64> {
        let clients = self.clients.lock().unwrap();
        match clients.get(client).and_then(|c| c.locked_until) {
            Some(until) if now < until => Err((until - now).num_seconds().max(1)),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&self, client: &str, now: DateTime<Utc>) {
        let mut clients = self.clients.lock().unwrap();
        let forget = Duration::minutes(Self::FORGET_AFTER_MINUTES);
        if clients
            .get(client)
            .is_some_and(|c| now - c.last_failure > forget)
        {
            clients.remove(client);
        }
        if !clients.contains_key(client) && clients.len() >= Self::MAX_CLIENTS {
            clients.retain(|_, c| now - c.last_failure <= forget);
            if clients.len() >= Self::MAX_CLIENTS
                && let Some(oldest) = clients
                    .iter()
                    .min_by_key(|(_, c)| c.last_failure)
                    .map(|(key, _)| key.clone())
            {
                clients.remove(&oldest);
            }
        }
        let state = clients.entry(client.to_owned()).or_insert(ClientState {
            failures: 0,
            last_failure: now,
            locked_until: None,
        });
        state.failures += 1;
        state.last_failure = now;
        if state.failures >= Self::FREE_ATTEMPTS {
            let doublings = (state.failures - Self::FREE_ATTEMPTS).min(10);
            let seconds = (Self::BASE_LOCK_SECONDS << doublings).min(Self::MAX_LOCK_SECONDS);
            state.locked_until = Some(now + Duration::seconds(seconds));
        }
    }

    /// Un succès remet à zéro ce client, et lui seul.
    pub fn record_success(&self, client: &str) {
        self.clients.lock().unwrap().remove(client);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(minutes: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 26, 12, 0, 0).unwrap() + Duration::minutes(minutes)
    }

    #[test]
    fn l_empreinte_d_un_jeton_est_stable_et_ne_contient_pas_le_jeton() {
        let hash = hash_token("abc");
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(!hash.contains("abc"));
    }

    #[test]
    fn une_session_expire_apres_la_duree_prevue() {
        let session = Session::open("jeton", at(0));
        assert!(session.is_valid_at(at(60 * 24 * 13)));
        assert!(!session.is_valid_at(at(60 * 24 * 14)));
    }

    #[test]
    fn un_mot_de_passe_court_ou_different_est_refuse_avec_un_message() {
        assert!(
            check_new_password("court", "court")
                .unwrap_err()
                .contains("12")
        );
        assert!(
            check_new_password("une phrase de passe", "une autre phrase")
                .unwrap_err()
                .contains("identiques")
        );
        assert!(check_new_password(&"a".repeat(300), &"a".repeat(300)).is_err());
        assert!(check_new_password("une phrase de passe", "une phrase de passe").is_ok());
    }

    const ME: &str = "203.0.113.7";
    const OTHER: &str = "198.51.100.9";

    #[test]
    fn la_porte_se_ferme_apres_cinq_echecs_puis_plus_longtemps() {
        let throttle = Throttle::default();
        for _ in 0..4 {
            throttle.record_failure(ME, at(0));
        }
        assert!(throttle.check(ME, at(0)).is_ok());

        throttle.record_failure(ME, at(0));
        assert_eq!(throttle.check(ME, at(0)), Err(60));
        assert!(throttle.check(ME, at(2)).is_ok());

        // Un sixième échec double l'attente ; le succès remet tout à zéro.
        throttle.record_failure(ME, at(2));
        assert_eq!(throttle.check(ME, at(2)), Err(120));
        throttle.record_success(ME);
        assert!(throttle.check(ME, at(2)).is_ok());
    }

    #[test]
    fn l_attente_est_plafonnee() {
        let throttle = Throttle::default();
        for _ in 0..40 {
            throttle.record_failure(ME, at(0));
        }
        assert_eq!(throttle.check(ME, at(0)), Err(15 * 60));
    }

    #[test]
    fn un_client_acharne_ne_bloque_pas_les_autres() {
        let throttle = Throttle::default();
        for _ in 0..20 {
            throttle.record_failure(OTHER, at(0));
        }
        assert!(throttle.check(OTHER, at(0)).is_err());
        assert!(throttle.check(ME, at(0)).is_ok());
        // Son succès à lui ne libère pas l'autre.
        throttle.record_success(ME);
        assert!(throttle.check(OTHER, at(0)).is_err());
    }

    #[test]
    fn un_client_qui_n_a_pas_echoue_depuis_une_heure_repart_de_zero() {
        let throttle = Throttle::default();
        for _ in 0..4 {
            throttle.record_failure(ME, at(0));
        }
        // Sans l'oubli, ce cinquième échec fermerait la porte.
        throttle.record_failure(ME, at(61));
        assert!(throttle.check(ME, at(61)).is_ok());
    }

    #[test]
    fn la_table_reste_bornee_et_retire_le_plus_ancien() {
        let throttle = Throttle::default();
        for i in 0..Throttle::MAX_CLIENTS {
            throttle.record_failure(&format!("client-{i}"), at(0) + Duration::seconds(i as i64));
        }
        throttle.record_failure("nouveau", at(1));
        let clients = throttle.clients.lock().unwrap();
        assert_eq!(clients.len(), Throttle::MAX_CLIENTS);
        assert!(clients.contains_key("nouveau"));
        assert!(!clients.contains_key("client-0"));
    }
}
