//! Empreintes de mot de passe en Argon2id (paramètres par défaut de la crate : 19 Mo, 2 passes), avec un sel
//! aléatoire propre à chaque empreinte. L'empreinte est au format PHC (`$argon2id$…`) : elle porte ses
//! propres paramètres, ce qui permet de les durcir plus tard sans casser les empreintes existantes.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher as _, PasswordVerifier};
use auth::PasswordHasher;

#[derive(Default)]
pub struct Argon2Hasher;

impl PasswordHasher for Argon2Hasher {
    fn hash(&self, password: &str) -> anyhow::Result<String> {
        // Le sel est tiré au hasard par la crate (getrandom).
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|e| anyhow::anyhow!("Calcul de l'empreinte impossible : {e}"))
    }

    fn verify(&self, password: &str, hash: &str) -> anyhow::Result<bool> {
        let parsed = PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Empreinte de mot de passe illisible : {e}"))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_empreinte_valide_le_bon_mot_de_passe_et_refuse_les_autres() {
        let hasher = Argon2Hasher;
        let hash = hasher.hash("une phrase de passe").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(!hash.contains("phrase"));
        assert!(hasher.verify("une phrase de passe", &hash).unwrap());
        assert!(!hasher.verify("une autre phrase", &hash).unwrap());
        // Deux empreintes du même mot de passe diffèrent (sel aléatoire).
        assert_ne!(hash, hasher.hash("une phrase de passe").unwrap());
        assert!(hasher.verify("x", "pas une empreinte").is_err());
    }
}
