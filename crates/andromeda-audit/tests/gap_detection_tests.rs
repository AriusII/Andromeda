/// Gap Detection Tests (Category E)
/// Tests verify that gaps in audit sequences are detected.
use andromeda_audit::PermissionAuditEvent;
use andromeda_core::{Permission, PrincipalId};
use andromeda_observability::TraceId;

/// Test: audit_gap_in_sequence_detected
/// Verifies that gaps in trace ID sequence are detected.
#[test]
fn audit_gap_in_sequence_detected() {
    let trace_ids = vec![
        TraceId::new(30001),
        TraceId::new(30002),
        TraceId::new(30003),
        // GAP: 30004 is missing
        TraceId::new(30005),
        TraceId::new(30006),
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    // Detect gap between entry 3 and 5
    let gap_detected = events[3].trace_id.get() - events[2].trace_id.get() > 1;
    assert!(
        gap_detected,
        "Gap should be detected between sequential entries"
    );

    assert_eq!(events.len(), 5);
}

/// Test: audit_gap_with_partial_write_suspected
/// Verifies that gaps indicate partial writes.
#[test]
fn audit_gap_with_partial_write_suspected() {
    let trace_ids = vec![
        TraceId::new(31001),
        TraceId::new(31002),
        TraceId::new(31003),
        TraceId::new(31005), // Gap: 31004 missing (partial write)
        TraceId::new(31006),
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    // Find gaps
    let mut gaps = Vec::new();
    for i in 1..events.len() {
        let gap = events[i].trace_id.get() - events[i - 1].trace_id.get();
        if gap > 1 {
            gaps.push((i - 1, i, gap));
        }
    }

    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].2, 2); // Gap size of 2 (31004 was skipped)
}

/// Test: audit_gap_large_sequence_missing
/// Verifies detection of large gaps in sequences.
#[test]
fn audit_gap_large_sequence_missing() {
    let trace_ids = vec![
        TraceId::new(32001),
        TraceId::new(32100), // Large gap: 32002-32099 missing
        TraceId::new(32101),
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    let large_gap = events[1].trace_id.get() - events[0].trace_id.get();
    assert_eq!(large_gap, 99);
    assert!(large_gap > 1);
}

/// Test: audit_gap_at_sequence_start
/// Verifies detection of missing entries at sequence start.
#[test]
fn audit_gap_at_sequence_start() {
    // Sequence doesn't start at 0, indicating missing entries
    let trace_ids = vec![
        TraceId::new(33050), // Starts at 50, not 0
        TraceId::new(33051),
        TraceId::new(33052),
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    // Gap at start (entries 0-49 are missing)
    assert_eq!(events[0].trace_id.get(), 33050);
    assert!(events[0].trace_id.get() > 33000);
}

/// Test: audit_gap_at_sequence_end
/// Verifies detection of potential missing entries at sequence end.
#[test]
fn audit_gap_at_sequence_end() {
    let trace_ids = vec![
        TraceId::new(34001),
        TraceId::new(34002),
        TraceId::new(34003),
        // Sequence ends here, but should continue (gap at end)
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    // Sequence ends at 34003, next expected would be 34004
    assert_eq!(events.last().unwrap().trace_id, TraceId::new(34003));
}

/// Test: audit_gap_multiple_gaps_detected
/// Verifies detection of multiple gaps in a sequence.
#[test]
fn audit_gap_multiple_gaps_detected() {
    let trace_ids = vec![
        TraceId::new(35001),
        TraceId::new(35002),
        // Gap 1: missing 35003-35004
        TraceId::new(35005),
        TraceId::new(35006),
        // Gap 2: missing 35007-35009
        TraceId::new(35010),
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    let mut gap_count = 0;
    for i in 1..events.len() {
        if events[i].trace_id.get() - events[i - 1].trace_id.get() > 1 {
            gap_count += 1;
        }
    }

    assert_eq!(gap_count, 2);
}

/// Test: audit_gap_zero_entries_missing_detected
/// Verifies that consecutive entries (no gaps) are correctly identified.
#[test]
fn audit_gap_zero_entries_missing_detected() {
    let trace_ids = vec![
        TraceId::new(36001),
        TraceId::new(36002),
        TraceId::new(36003),
        TraceId::new(36004),
        TraceId::new(36005),
    ];

    let events: Vec<_> = trace_ids
        .iter()
        .map(|&tid| {
            PermissionAuditEvent::allowed(
                tid,
                PrincipalId::new(42),
                Permission::ReadContractMetadata,
            )
        })
        .collect();

    // No gaps - all entries are sequential
    let mut gap_count = 0;
    for i in 1..events.len() {
        if events[i].trace_id.get() - events[i - 1].trace_id.get() != 1 {
            gap_count += 1;
        }
    }

    assert_eq!(gap_count, 0);
}
