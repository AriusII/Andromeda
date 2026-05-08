use andromeda_core::AndromedaResult;
use sha2::{Digest, Sha256};

use super::super::{
    artifacts::{BackupArtifactDigest, BackupWalSegmentArtifact},
    helpers::{backup_error, map_backup_validation},
};

pub(super) fn digest_bytes(bytes: &[u8]) -> AndromedaResult<BackupArtifactDigest> {
    if bytes.is_empty() {
        return Err(backup_error(
            "backup artifact digest bytes must not be empty",
        ));
    }
    let sha256: [u8; 32] = Sha256::digest(bytes).into();
    let digest = BackupArtifactDigest {
        sha256,
        crc64: crc64(bytes),
        byte_len: u64::try_from(bytes.len())
            .map_err(|_| backup_error("backup artifact byte length exceeds u64"))?,
    };
    map_backup_validation(digest.validate("backup artifact digest"))?;
    Ok(digest)
}

pub(super) fn compute_wal_archive_digest(wal_segments: &[BackupWalSegmentArtifact]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for segment in wal_segments {
        hasher.update(segment.segment_id.to_le_bytes());
        hasher.update(segment.first_lsn.get().to_le_bytes());
        hasher.update(segment.last_lsn.get().to_le_bytes());
        match segment.base_previous_lsn {
            Some(previous) => {
                hasher.update([1]);
                hasher.update(previous.get().to_le_bytes());
            }
            None => {
                hasher.update([0]);
                hasher.update(0_u64.to_le_bytes());
            }
        }
        hasher.update(segment.record_count.to_le_bytes());
        hasher.update(segment.wal_format_version.to_le_bytes());
        hasher.update(segment.artifact.sha256);
        hasher.update(segment.artifact.crc64.to_le_bytes());
        hasher.update(segment.artifact.byte_len.to_le_bytes());
    }
    hasher.finalize().into()
}

fn crc64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}
