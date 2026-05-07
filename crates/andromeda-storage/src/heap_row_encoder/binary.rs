use andromeda_core::AndromedaResult;

use super::error::encoder_error;

pub(crate) fn read_scalar_byte(bytes: &[u8], scalar_name: &'static str) -> AndromedaResult<u8> {
    bytes
        .first()
        .copied()
        .ok_or_else(|| encoder_error(format!("insufficient bytes for {}", scalar_name)))
}

pub(crate) fn read_scalar_array<const N: usize>(
    bytes: &[u8],
    scalar_name: &'static str,
) -> AndromedaResult<[u8; N]> {
    if bytes.len() < N {
        return Err(encoder_error(format!(
            "insufficient bytes for {}",
            scalar_name
        )));
    }
    let mut result = [0; N];
    result.copy_from_slice(&bytes[..N]);
    Ok(result)
}

pub(crate) fn read_array_at<const N: usize>(
    bytes: &[u8],
    offset: usize,
    err_msg: &str,
) -> AndromedaResult<[u8; N]> {
    if offset + N > bytes.len() {
        return Err(encoder_error(err_msg));
    }
    let mut result = [0; N];
    result.copy_from_slice(&bytes[offset..offset + N]);
    Ok(result)
}
