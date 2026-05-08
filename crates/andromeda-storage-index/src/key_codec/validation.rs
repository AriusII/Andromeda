use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::{KeyDatum, KeyScalarType};

pub(crate) fn validate_composite_datum_type(
    index: usize,
    datum: &KeyDatum,
    scalar_type: KeyScalarType,
) -> AndromedaResult<()> {
    let compatible = matches!(datum, KeyDatum::Null) || datum.matches_scalar_type(scalar_type);

    if compatible {
        return Ok(());
    }

    match datum {
        KeyDatum::Text(_) | KeyDatum::Bytes(_) => Err(codec_error(format!(
            "composite schema cannot validate variable-width datum at column {} without Text/Bytes scalar type",
            index
        ))),
        _ => Err(codec_error(format!(
            "composite schema type mismatch at column {}: datum {:?} is not compatible with {:?}",
            index, datum, scalar_type
        ))),
    }
}

pub(crate) fn checked_u16_len(len: usize, err_msg: &'static str) -> AndromedaResult<u16> {
    if len > u16::MAX as usize {
        return Err(codec_error(err_msg));
    }
    Ok(len as u16)
}

pub(crate) fn checked_end(
    bytes: &[u8],
    position: usize,
    len: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<usize> {
    let end = position
        .checked_add(len)
        .ok_or_else(|| codec_error("key payload offset overflow"))?;
    if end > bytes.len() {
        return Err(codec_error(truncated_msg));
    }
    Ok(end)
}

pub(crate) fn codec_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
