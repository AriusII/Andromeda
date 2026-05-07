use andromeda_observe::{TraceQueryFilter, TraceQueryPermissionMatrix};

use super::super::{AuditCompactReport, AuditInspectionReport, AuditVerifyReport};
use super::{
    format::{audit_output_text, option_u64_human},
    labels::{permission_str, surface_str, trace_family_str},
    records::print_trace_row_human,
};

pub(super) fn print_audit_inspection_human(report: &AuditInspectionReport) {
    println!("Audit Replay Inspection");
    println!("=======================");
    println!("Schema: {}", report.schema);
    println!("Contract Preview: {}", report.contract_preview);
    println!("Durable Backend Wired: {}", report.durable_backend);
    println!(
        "Requires Durable Audit Journal: {}",
        report.requires_durable_audit_journal
    );
    if let Some(path) = &report.journal_path {
        println!("Journal: {}", audit_output_text(path));
    }
    print_permission_human(TraceQueryPermissionMatrix::V1_ADMIN);
    println!("Limit: {}", report.spec.limit);
    println!("Offset: {}", report.spec.offset);
    print_filters_human(&report.spec.filter);
    if let Some(result) = &report.result {
        println!("Returned Rows: {}", result.metadata.returned_rows);
        println!("Truncated: {}", result.metadata.truncated);
        for row in &result.rows {
            print_trace_row_human(row);
        }
    }
    if let Some(evidence) = report.diagnostic_evidence {
        println!("Records Scanned: {}", evidence.records_scanned);
        println!("Records Matched: {}", evidence.records_matched);
        println!("Records Returned: {}", evidence.records_returned);
        println!("Filter Applied: {}", evidence.filter_applied);
        println!("Evidence Limit: {}", evidence.limit);
        println!("Evidence Offset: {}", evidence.offset);
        println!("Evidence Truncated: {}", evidence.truncated);
    }
    println!("{}", audit_output_text(&report.message));
}

pub(super) fn print_audit_compact_human(report: &AuditCompactReport) {
    println!("Audit Compact");
    println!("=============");
    println!("Schema: {}", report.schema);
    println!("Contract Preview: {}", report.contract_preview);
    println!("Durable Backend Wired: {}", report.durable_backend);
    println!(
        "Requires Durable Audit Journal: {}",
        report.requires_durable_audit_journal
    );
    if let Some(path) = &report.journal_path {
        println!("Journal: {}", audit_output_text(path));
    }
    println!("Retain From LSN: {}", report.retain_from_lsn);
    println!("Preserve Forensic Hold: {}", report.preserve_forensic_hold);
    if let Some(compaction) = report.report {
        println!("Records Scanned: {}", compaction.records_scanned);
        println!("Records Retained: {}", compaction.records_retained);
        println!("Records Expired: {}", compaction.records_expired);
        println!(
            "First Retained LSN: {}",
            option_u64_human(compaction.first_retained_lsn)
        );
        println!(
            "Last Retained LSN: {}",
            option_u64_human(compaction.last_retained_lsn)
        );
        println!(
            "Retained Checksum Evidence: {}",
            compaction.retained_checksum_evidence
        );
    }
    println!("{}", audit_output_text(&report.message));
}

pub(super) fn print_audit_verify_human(report: &AuditVerifyReport) {
    println!("Audit Verify");
    println!("============");
    println!("Schema: {}", report.schema);
    println!("Durable Backend Wired: {}", report.durable_backend);
    println!(
        "Requires Durable Audit Journal: {}",
        report.requires_durable_audit_journal
    );
    println!("Journal: {}", audit_output_text(&report.journal_path));
    println!("Records Scanned: {}", report.records_scanned);
    println!("Records Returned: {}", report.records_returned);
    println!(
        "First Returned LSN: {}",
        option_u64_human(report.first_returned_lsn)
    );
    println!(
        "Last Returned LSN: {}",
        option_u64_human(report.last_returned_lsn)
    );
    println!("{}", audit_output_text(&report.message));
}

fn print_permission_human(matrix: TraceQueryPermissionMatrix) {
    println!(
        "Permission: {} on {} (audit required: {})",
        permission_str(matrix.required_permission),
        surface_str(matrix.surface),
        matrix.audit_required
    );
}

fn print_filters_human(filter: &TraceQueryFilter) {
    if let Some(trace_id) = filter.trace_id {
        println!("Trace ID: {}", trace_id.get());
    }
    if let Some(principal) = &filter.principal {
        println!("Principal: {}", audit_output_text(principal));
    }
    if let Some(family) = filter.family {
        println!("Family: {}", trace_family_str(family));
    }
    if let Some(range) = filter.lsn_range {
        println!("LSN Range: {}..={}", range.start_lsn, range.end_lsn);
    }
}
