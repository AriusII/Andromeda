use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash};

pub(crate) fn validate_optional_contract_hash(
    label: &str,
    bytes: Option<&[u8]>,
) -> AndromedaResult<()> {
    if let Some(bytes) = bytes {
        validate_required_contract_hash(label, bytes)?;
    }

    Ok(())
}

pub(crate) fn validate_required_contract_hash(label: &str, bytes: &[u8]) -> AndromedaResult<()> {
    if bytes.len() != ContractHash::LEN {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} must be 32 bytes"),
        ));
    }

    if bytes.iter().all(|byte| *byte == 0) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            format!("{label} must not be zero"),
        ));
    }

    Ok(())
}
