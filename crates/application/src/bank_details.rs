//! La banque et le numéro d'un compte bancaire : vérifiés de la même façon à l'ajout et à la modification.

use asset::{Asset, AssetKind};

use crate::UseCaseError;

/// La banque et le numéro nettoyés. Seul un compte bancaire en a : pour tout autre type, ils sont ignorés
/// (un livret ou un prêt dont on change le type ne garde pas un numéro qui n'a plus de sens).
pub(crate) fn bank_details(
    kind: AssetKind,
    bank_name: &str,
    account_number: &str,
) -> Result<(Option<String>, Option<String>), UseCaseError> {
    if kind != AssetKind::BankAccount {
        return Ok((None, None));
    }
    let bank_name = Asset::validate_bank_name(bank_name).map_err(UseCaseError::Refusal)?;
    let account_number =
        Asset::validate_account_number(account_number).map_err(UseCaseError::Refusal)?;
    Ok((bank_name, account_number))
}
