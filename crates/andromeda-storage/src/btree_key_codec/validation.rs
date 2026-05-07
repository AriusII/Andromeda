use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::heap_row_encoder::{Datum, ScalarType};

pub(super) fn validate_composite_datum_type(
    idx: usize,
    datum: &Datum,
    scalar_type: ScalarType,
) -> AndromedaResult<()> {
    let compatible = matches!(
        (datum, scalar_type),
        (Datum::Null, _)
            | (Datum::Int32(_), ScalarType::Int32)
            | (Datum::Int64(_), ScalarType::Int64)
            | (Datum::Bool(_), ScalarType::Bool)
    );

    if compatible {
        return Ok(());
    }

    match datum {
        Datum::Text(_) | Datum::Bytes(_) => Err(codec_error(format!(
            "composite schema cannot validate variable-width datum at column {} without Text/Bytes scalar type",
            idx
        ))),
        _ => Err(codec_error(format!(
            "composite schema type mismatch at column {}: datum {:?} is not compatible with {:?}",
            idx, datum, scalar_type
        ))),
    }
}

pub(super) fn checked_u16_len(len: usize, err_msg: &'static str) -> AndromedaResult<u16> {
    if len > u16::MAX as usize {
        return Err(codec_error(err_msg));
    }
    Ok(len as u16)
}

pub(super) fn checked_end(
    bytes: &[u8],
    pos: usize,
    len: usize,
    truncated_msg: &'static str,
) -> AndromedaResult<usize> {
    let end = pos
        .checked_add(len)
        .ok_or_else(|| codec_error("key payload offset overflow"))?;
    if end > bytes.len() {
        return Err(codec_error(truncated_msg));
    }
    Ok(end)
}

pub(super) fn codec_error(msg: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, msg)
}
