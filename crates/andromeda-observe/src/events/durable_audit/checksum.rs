use andromeda_core::digest::sha256;

pub(super) fn checksum64(bytes: &[u8]) -> u64 {
    let digest = sha256(bytes);
    let checksum = u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]);
    checksum.max(1)
}
