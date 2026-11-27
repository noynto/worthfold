//! La banque et le numéro d'un compte bancaire : vérifiés de la même façon à l'ajout et à la modification.

use asset::{Asset, AssetKind};
use institution::InstitutionProvider;

use crate::UseCaseError;

/// La banque et le numéro nettoyés. Seul un compte bancaire en a : pour tout autre type, ils sont ignorés
/// (un livret ou un prêt dont on change le type ne garde pas un numéro qui n'a plus de sens).
/// La banque doit exister : c'est elle qui dit comment lire les relevés du compte.
pub(crate) async fn bank_details(
    institutions: &dyn InstitutionProvider,
    kind: AssetKind,
    bank_name: &str,
    account_number: &str,
) -> Result<(Option<String>, Option<String>), UseCaseError> {
    if kind != AssetKind::BankAccount {
        return Ok((None, None));
    }
    let bank_name = match Asset::validate_bank_name(bank_name).map_err(UseCaseError::Refusal)? {
        None => None,
        Some(name) => Some(
            institutions
                .read(&name)
                .await?
                .ok_or_else(|| {
                    UseCaseError::refusal(format!(
                        "La banque « {name} » n'existe pas : ajoute-la d'abord dans les banques."
                    ))
                })?
                .name,
        ),
    };
    let account_number =
        Asset::validate_account_number(account_number).map_err(UseCaseError::Refusal)?;
    Ok((bank_name, account_number))
}
