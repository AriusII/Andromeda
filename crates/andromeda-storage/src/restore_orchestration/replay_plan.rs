use andromeda_core::AndromedaResult;

use crate::{Lsn, WalSegmentDescriptor, backup::BackupManifest};

use super::{error::restore_error, validation::validate_restore_prerequisites};

/// WAL segment identified for replay during PITR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalSegmentToReplay {
    /// Descriptor of the segment to replay
    pub segment_descriptor: WalSegmentDescriptor,

    /// Position in replay sequence (0 = first segment)
    pub sequence_index: usize,

    /// Whether this segment contains or passes the PITR target LSN
    pub contains_pitr_target: bool,

    /// Last LSN the replay executor may apply from this segment.
    ///
    /// For segments before the target this is the segment's last LSN. For the
    /// segment containing the PITR target this is exactly the requested target
    /// LSN, so replay does not advance to the segment tail by accident.
    pub replay_stop_lsn: Lsn,
}

/// Plan WAL segment replay sequence to reach PITR target LSN.
///
/// Pure function; no I/O or async.
///
/// Input:
/// - Manifest with cold snapshot + WAL archive range
/// - PITR target LSN (already validated to be in range)
/// - WAL segment descriptors (from archive)
///
/// Output:
/// - Ordered list of segments to replay
/// - Validation: no gaps in LSN chain, contiguity maintained
pub fn plan_replay_segments(
    manifest: &BackupManifest,
    pitr_lsn: Lsn,
    segments: &[WalSegmentDescriptor],
) -> AndromedaResult<Vec<WalSegmentToReplay>> {
    validate_restore_prerequisites(manifest, pitr_lsn)?;

    if pitr_lsn == manifest.snapshot.base_checkpoint_lsn {
        return Ok(Vec::new());
    }

    validate_first_replay_segment(manifest, segments)?;

    let mut replay_plan: Vec<WalSegmentToReplay> = Vec::new();
    let mut expected_previous_lsn: Option<Lsn> = None;

    for (index, seg) in segments.iter().enumerate() {
        seg.validate()?;
        validate_replay_chain_link(seg, expected_previous_lsn)?;
        validate_replay_segment_bounds(manifest, seg)?;
        let contains_pitr_target = seg.first_lsn <= pitr_lsn && pitr_lsn <= seg.last_lsn;

        replay_plan.push(replay_segment_for(
            *seg,
            index,
            contains_pitr_target,
            pitr_lsn,
        ));

        expected_previous_lsn = Some(seg.last_lsn);

        if contains_pitr_target {
            break;
        }
    }

    if !replay_plan.iter().any(|seg| seg.contains_pitr_target) {
        return Err(restore_error("no WAL segment contains the PITR target LSN"));
    }

    Ok(replay_plan)
}

fn validate_first_replay_segment(
    manifest: &BackupManifest,
    segments: &[WalSegmentDescriptor],
) -> AndromedaResult<()> {
    let first = segments
        .first()
        .ok_or_else(|| restore_error("WAL segment list must not be empty"))?;
    if first.first_lsn != manifest.wal_archive.start {
        return Err(restore_error(
            "first WAL segment must start at backup archive start LSN",
        ));
    }

    Ok(())
}

fn validate_replay_chain_link(
    segment: &WalSegmentDescriptor,
    expected_previous_lsn: Option<Lsn>,
) -> AndromedaResult<()> {
    if let Some(expected_prev) = expected_previous_lsn {
        if segment.base_previous_lsn != Some(expected_prev) {
            return Err(restore_error(
                "WAL segment chain has gap: base_previous_lsn mismatch",
            ));
        }
    } else if segment.base_previous_lsn.is_some() {
        return Err(restore_error(
            "first WAL segment must have base_previous_lsn = None",
        ));
    }

    Ok(())
}

fn validate_replay_segment_bounds(
    manifest: &BackupManifest,
    segment: &WalSegmentDescriptor,
) -> AndromedaResult<()> {
    if segment.first_lsn < manifest.wal_archive.start
        || segment.last_lsn > manifest.wal_archive.end_inclusive
    {
        return Err(restore_error(
            "WAL segment LSN bounds exceed backup archive range",
        ));
    }

    Ok(())
}

fn replay_segment_for(
    segment: WalSegmentDescriptor,
    sequence_index: usize,
    contains_pitr_target: bool,
    pitr_lsn: Lsn,
) -> WalSegmentToReplay {
    WalSegmentToReplay {
        replay_stop_lsn: if contains_pitr_target {
            pitr_lsn
        } else {
            segment.last_lsn
        },
        segment_descriptor: segment,
        sequence_index,
        contains_pitr_target,
    }
}
