use andromeda_core::AndromedaResult;

use crate::{DatabaseManifest, Lsn, WalRecord, WalRecordKind};

use super::super::storage_error;
use super::{ManifestSwitchRecoveryTrace, ReplayContext, ReplayResult};

/// Replay manifest switch record.
///
/// **Idempotency:** Switching to the same manifest twice is a no-op.
/// **Invariant:** Manifest switch must be atomic with respect to other operations.
pub(super) fn replay_manifest_switch(
    ctx: &mut ReplayContext,
    record: &WalRecord,
) -> AndromedaResult<ReplayResult> {
    let payload = parse_manifest_switch_payload(record.payload())?;

    if payload.required_wal_start_lsn < payload.base_checkpoint_lsn {
        ctx.manifest_switch_traces.push(
            ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                lsn: record.header.lsn,
                manifest_version: payload.manifest_version,
                reason: "required_wal_start_lsn precedes base_checkpoint_lsn",
            },
        );
        return Ok(ReplayResult::skipped(
            record.header.lsn,
            WalRecordKind::ManifestSwitch,
        ));
    }

    if payload.base_checkpoint_lsn > record.header.lsn {
        ctx.manifest_switch_traces.push(
            ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                lsn: record.header.lsn,
                manifest_version: payload.manifest_version,
                reason: "base_checkpoint_lsn exceeds manifest switch record LSN",
            },
        );
        return Ok(ReplayResult::skipped(
            record.header.lsn,
            WalRecordKind::ManifestSwitch,
        ));
    }

    if ctx.require_checkpoint_end_for_manifest_switch
        && !payload.base_checkpoint_lsn.is_zero()
        && ctx
            .latest_checkpoint_end_lsn
            .is_none_or(|checkpoint_lsn| checkpoint_lsn < payload.base_checkpoint_lsn)
    {
        ctx.manifest_switch_traces.push(
            ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                lsn: record.header.lsn,
                manifest_version: payload.manifest_version,
                reason: "base_checkpoint_lsn lacks durable checkpoint_end evidence",
            },
        );
        return Ok(ReplayResult::skipped(
            record.header.lsn,
            WalRecordKind::ManifestSwitch,
        ));
    }

    let expected_crc = ctx
        .known_manifest_crc_by_version
        .get(&payload.manifest_version)
        .copied()
        .or_else(|| {
            ctx.active_manifest
                .filter(|manifest| manifest.manifest_version == payload.manifest_version)
                .map(|manifest| manifest.manifest_crc)
        });

    if matches!(expected_crc, Some(expected) if expected != payload.manifest_crc) {
        ctx.manifest_switch_traces.push(
            ManifestSwitchRecoveryTrace::ManifestSwitchValidationFailed {
                lsn: record.header.lsn,
                manifest_version: payload.manifest_version,
                reason: "manifest CRC mismatch",
            },
        );
        return Ok(ReplayResult::skipped(
            record.header.lsn,
            WalRecordKind::ManifestSwitch,
        ));
    }

    let next_manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: payload.manifest_version,
        snapshot_id: payload.snapshot_id,
        base_checkpoint_lsn: payload.base_checkpoint_lsn,
        required_wal_start_lsn: payload.required_wal_start_lsn,
        previous_manifest_hash: payload.previous_manifest_hash,
        manifest_crc: payload.manifest_crc,
    };
    next_manifest.validate()?;

    ctx.known_manifest_crc_by_version
        .insert(payload.manifest_version, payload.manifest_crc);
    ctx.active_manifest = Some(next_manifest);
    ctx.manifest_switch_traces
        .push(ManifestSwitchRecoveryTrace::ManifestSwitchApplied {
            lsn: record.header.lsn,
            manifest_version: payload.manifest_version,
            snapshot_id: payload.snapshot_id,
            base_checkpoint_lsn: payload.base_checkpoint_lsn,
            required_wal_start_lsn: payload.required_wal_start_lsn,
        });

    Ok(ReplayResult::applied(
        record.header.lsn,
        WalRecordKind::ManifestSwitch,
    ))
}

#[cfg(test)]
pub(super) const MANIFEST_SWITCH_PAYLOAD_LEN: usize = 68;

#[cfg(not(test))]
const MANIFEST_SWITCH_PAYLOAD_LEN: usize = 68;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManifestSwitchPayload {
    manifest_version: u64,
    snapshot_id: u64,
    base_checkpoint_lsn: Lsn,
    required_wal_start_lsn: Lsn,
    previous_manifest_hash: [u8; 32],
    manifest_crc: u32,
}

fn parse_manifest_switch_payload(bytes: &[u8]) -> AndromedaResult<ManifestSwitchPayload> {
    if bytes.len() != MANIFEST_SWITCH_PAYLOAD_LEN {
        return Err(storage_error(
            "manifest switch payload length must be exactly 68 bytes",
        ));
    }

    fn read_u64(bytes: &[u8], start: usize) -> AndromedaResult<u64> {
        let end = start + 8;
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| storage_error("manifest switch payload is truncated"))?;
        let mut array = [0u8; 8];
        array.copy_from_slice(slice);
        Ok(u64::from_le_bytes(array))
    }
    fn read_u32(bytes: &[u8], start: usize) -> AndromedaResult<u32> {
        let end = start + 4;
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| storage_error("manifest switch payload is truncated"))?;
        let mut array = [0u8; 4];
        array.copy_from_slice(slice);
        Ok(u32::from_le_bytes(array))
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(
        bytes
            .get(32..64)
            .ok_or_else(|| storage_error("manifest switch payload hash is truncated"))?,
    );

    Ok(ManifestSwitchPayload {
        manifest_version: read_u64(bytes, 0)?,
        snapshot_id: read_u64(bytes, 8)?,
        base_checkpoint_lsn: Lsn::new(read_u64(bytes, 16)?),
        required_wal_start_lsn: Lsn::new(read_u64(bytes, 24)?),
        previous_manifest_hash: hash,
        manifest_crc: read_u32(bytes, 64)?,
    })
}
