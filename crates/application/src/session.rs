//! Ce que les cas d'usage d'authentification ont en commun : le jeton de session et le message d'attente.

/// Un jeton de session : deux valeurs aléatoires de 122 bits, concaténées (244 bits, imprévisible).
pub(crate) fn new_token() -> String {
    format!("{}{}", shared::random_token(), shared::random_token())
}

pub(crate) fn too_many_attempts(seconds: i64) -> String {
    if seconds >= 120 {
        format!(
            "Trop d'essais : réessaie dans {} minutes.",
            (seconds + 59) / 60
        )
    } else {
        format!("Trop d'essais : réessaie dans {seconds} secondes.")
    }
}
