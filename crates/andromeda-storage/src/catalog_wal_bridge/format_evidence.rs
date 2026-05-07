use sha2::{Digest, Sha256};

pub(super) const VERSION_BYTES: usize = 2;
pub(super) const PAYLOAD_LEN_BYTES: usize = 4;
pub(super) const CHECKSUM_BYTES: usize = 32;
pub(super) const FRAME_HEADER_BYTES: usize = VERSION_BYTES + PAYLOAD_LEN_BYTES + CHECKSUM_BYTES;

pub(super) type CatalogWalChecksum = [u8; CHECKSUM_BYTES];

pub(super) fn payload_checksum(payload: &[u8]) -> CatalogWalChecksum {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().into()
}

pub(super) fn checked_frame_len(payload_len: usize) -> Option<usize> {
    FRAME_HEADER_BYTES.checked_add(payload_len)
}

/// Helper: format checksum as hex string for logging.
pub(super) fn format_checksum(checksum: &CatalogWalChecksum) -> String {
    checksum
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}
