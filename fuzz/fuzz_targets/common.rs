#![allow(
    dead_code,
    reason = "Fuzz targets share this module, but each bin uses a different subset."
)]

pub const MAX_4K_PAYLOAD_BYTES: usize = 4 * 1024;
pub const MAX_64K_INPUT_BYTES: usize = 64 * 1024;

pub fn bounded_input(data: &[u8], max_len: usize) -> &[u8] {
    &data[..data.len().min(max_len)]
}

pub fn decode_bounded<T, E>(
    data: &[u8],
    max_len: usize,
    decode: impl FnOnce(&[u8]) -> Result<T, E>,
) -> Option<T> {
    decode(bounded_input(data, max_len)).ok()
}

pub fn ignore_decode<T, E>(data: &[u8], decode: impl FnOnce(&[u8]) -> Result<T, E>) {
    let _ = decode(data);
}
