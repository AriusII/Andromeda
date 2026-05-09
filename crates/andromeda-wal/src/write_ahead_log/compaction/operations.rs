use andromeda_error::AndromedaResult;

use super::{CompactionContext, CompactionResult, FragmentationMetrics, WalCompactionAuditEvent};

/// Identify WAL segments that exceed the fragmentation threshold and are compaction candidates.
///
/// # Arguments
///
/// * `context` - Compaction context providing segment metrics
/// * `threshold_ratio` - Fragmentation threshold (e.g., 0.30 for 30%)
///
/// # Returns
///
/// Vector of `FragmentationMetrics` for candidates, sorted by fragmentation_ratio descending.
/// Higher fragmentation candidates are prioritized for compaction.
///
/// # Errors
///
/// Returns `Storage` error if segment metrics cannot be retrieved.
pub fn identify_compaction_candidates(
    context: &dyn CompactionContext,
    threshold_ratio: f64,
) -> AndromedaResult<Vec<FragmentationMetrics>> {
    let mut candidates = context.identify_fragmented_segments(threshold_ratio)?;

    // Sort by fragmentation_ratio descending (most fragmented first)
    candidates.sort_by(|a, b| {
        b.fragmentation_ratio
            .partial_cmp(&a.fragmentation_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Emit audit events for each candidate
    for metrics in &candidates {
        context.emit_audit_event(WalCompactionAuditEvent::CandidateIdentified {
            segment_id: metrics.segment_id,
            fragmentation_ratio: metrics.fragmentation_ratio,
            total_bytes: metrics.total_bytes,
            dead_bytes: metrics.dead_bytes,
        })?;
    }

    Ok(candidates)
}

/// Compact a single WAL segment by rewriting live records to a new segment.
///
/// # Algorithm
///
/// 1. Read all records from the segment
/// 2. Filter by liveness: keep only records that should be retained
/// 3. Write filtered records to a new temporary segment
/// 4. Atomically swap old segment with new segment
/// 5. On swap failure, abandon temporary and keep original
///
/// # Arguments
///
/// * `context` - Compaction context
/// * `segment_id` - ID of segment to compact
///
/// # Returns
///
/// `CompactionResult` if compaction succeeded, `Err` if compaction failed.
/// On error, old segment remains intact.
///
/// # Errors
///
/// Returns `Storage` error if:
/// - Segment cannot be read
/// - Record filtering fails
/// - Compacted segment write fails (temporary abandoned)
/// - Swap fails (old segment remains; temporary abandoned)
pub fn compact_segment(
    context: &dyn CompactionContext,
    segment_id: u64,
) -> AndromedaResult<CompactionResult> {
    // Step 1: Read all records from original segment
    let original_records = context.read_segment_records(segment_id)?;
    let original_record_count = original_records.len() as u64;
    let original_bytes = original_records
        .iter()
        .map(|r| r.payload.len() as u64)
        .sum::<u64>();

    context.emit_audit_event(WalCompactionAuditEvent::CompactionStarted {
        segment_id,
        fragmentation_ratio: 0.0, // Caller should provide this
    })?;

    // Step 2: Filter records by liveness
    let mut live_records = Vec::new();
    for record in &original_records {
        match context.should_keep_record(record) {
            Ok(true) => live_records.push(record.clone()),
            Ok(false) => {}, // Skip dead record
            Err(e) => {
                context.emit_audit_event(WalCompactionAuditEvent::CompactionFailed {
                    segment_id,
                    reason: format!("Record filtering error: {}", e),
                })?;
                return Err(e);
            },
        }
    }

    let compacted_record_count = live_records.len() as u64;
    let records_removed = original_record_count - compacted_record_count;

    // Step 3: Write compacted segment
    let (new_segment_id, compacted_bytes) =
        match context.write_compacted_segment(segment_id, &live_records) {
            Ok((id, size)) => (id, size),
            Err(e) => {
                context.emit_audit_event(WalCompactionAuditEvent::CompactionFailed {
                    segment_id,
                    reason: format!("Write failed: {}", e),
                })?;
                return Err(e);
            },
        };

    let bytes_recovered = original_bytes.saturating_sub(compacted_bytes);

    // Step 4: Atomically swap old ↔ new
    match context.swap_segment(segment_id, new_segment_id) {
        Ok(()) => {
            let result = CompactionResult {
                original_segment_id: segment_id,
                new_segment_id,
                original_bytes,
                compacted_bytes,
                bytes_recovered,
                original_record_count,
                compacted_record_count,
                records_removed,
            };

            context.emit_audit_event(WalCompactionAuditEvent::CompactionCompleted {
                original_segment_id: segment_id,
                new_segment_id,
                bytes_recovered,
                reduction_ratio: result.reduction_ratio(),
            })?;

            Ok(result)
        },
        Err(e) => {
            // Step 5: On swap failure, old segment remains and temporary is abandoned
            context.emit_audit_event(WalCompactionAuditEvent::CompactionFailed {
                segment_id,
                reason: format!("Swap failed: {}", e),
            })?;
            Err(e)
        },
    }
}
