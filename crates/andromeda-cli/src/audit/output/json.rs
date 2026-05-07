use andromeda_observe::{
    DurableAuditCompactionReport, DurableAuditTraceQueryResult, TraceQueryFilter,
    TraceQueryLsnRange, TraceQueryPermissionMatrix, TraceQuerySpec,
};

use super::super::{
    AuditCompactReport, AuditInspectionDiagnosticEvidence, AuditInspectionReport, AuditVerifyReport,
};
use super::{
    format::{json_audit_string, json_option_audit_string, json_optional_u64, json_raw_string},
    labels::{admin_operation_str, permission_str, surface_str, trace_family_str},
    records::rows_json,
};

pub(super) fn print_audit_inspection_json(report: &AuditInspectionReport) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"contract_preview\":{},\"durable_backend\":{},\"requires_durable_audit_journal\":{},\"journal_path\":{},\"inspection\":{},\"permission_matrix\":{},\"diagnostic_evidence\":{},\"result\":{},\"message\":{}}}",
        json_raw_string(report.schema),
        report.contract_preview,
        report.durable_backend,
        report.requires_durable_audit_journal,
        json_option_audit_string(report.journal_path.as_deref()),
        inspection_spec_json(&report.spec),
        permission_matrix_json(TraceQueryPermissionMatrix::V1_ADMIN),
        inspection_evidence_json(report.diagnostic_evidence.as_ref()),
        result_json(report.result.as_ref()),
        json_audit_string(&report.message),
    );
}

pub(super) fn print_audit_compact_json(report: &AuditCompactReport) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"contract_preview\":{},\"durable_backend\":{},\"requires_durable_audit_journal\":{},\"journal_path\":{},\"policy\":{{\"retain_from_lsn\":{},\"preserve_forensic_hold\":{}}},\"report\":{},\"message\":{}}}",
        json_raw_string(report.schema),
        report.contract_preview,
        report.durable_backend,
        report.requires_durable_audit_journal,
        json_option_audit_string(report.journal_path.as_deref()),
        report.retain_from_lsn,
        report.preserve_forensic_hold,
        compaction_report_json(report.report.as_ref()),
        json_audit_string(&report.message),
    );
}

pub(super) fn print_audit_verify_json(report: &AuditVerifyReport) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"durable_backend\":{},\"requires_durable_audit_journal\":{},\"journal_path\":{},\"records_scanned\":{},\"records_returned\":{},\"first_returned_lsn\":{},\"last_returned_lsn\":{},\"message\":{}}}",
        json_raw_string(report.schema),
        report.durable_backend,
        report.requires_durable_audit_journal,
        json_audit_string(&report.journal_path),
        report.records_scanned,
        report.records_returned,
        json_optional_u64(report.first_returned_lsn),
        json_optional_u64(report.last_returned_lsn),
        json_audit_string(&report.message),
    );
}

fn inspection_spec_json(spec: &TraceQuerySpec) -> String {
    format!(
        "{{\"limit\":{},\"offset\":{},\"include_total_count\":{},\"filters\":{}}}",
        spec.limit,
        spec.offset,
        spec.include_total_count,
        filters_json(&spec.filter)
    )
}

fn filters_json(filter: &TraceQueryFilter) -> String {
    format!(
        "{{\"trace_id\":{},\"principal\":{},\"family\":{},\"lsn_range\":{}}}",
        filter
            .trace_id
            .map(|trace_id| trace_id.get().to_string())
            .unwrap_or_else(|| "null".to_string()),
        json_option_audit_string(filter.principal.as_deref()),
        filter
            .family
            .map(|family| json_raw_string(trace_family_str(family)))
            .unwrap_or_else(|| "null".to_string()),
        lsn_range_json(filter.lsn_range),
    )
}

fn lsn_range_json(range: Option<TraceQueryLsnRange>) -> String {
    range
        .map(|range| {
            format!(
                "{{\"start_lsn\":{},\"end_lsn\":{}}}",
                range.start_lsn, range.end_lsn
            )
        })
        .unwrap_or_else(|| "null".to_string())
}

fn permission_matrix_json(matrix: TraceQueryPermissionMatrix) -> String {
    format!(
        "{{\"surface\":{},\"required_permission\":{},\"audit_operation\":{},\"audit_required\":{}}}",
        json_raw_string(surface_str(matrix.surface)),
        json_raw_string(permission_str(matrix.required_permission)),
        json_raw_string(admin_operation_str(matrix.audit_operation)),
        matrix.audit_required
    )
}

fn result_json(result: Option<&DurableAuditTraceQueryResult>) -> String {
    let Some(result) = result else {
        return "null".to_string();
    };
    format!(
        "{{\"metadata\":{{\"limit\":{},\"offset\":{},\"returned_rows\":{},\"total_matching_rows\":{},\"truncated\":{},\"ordered_by_event_id_ascending\":{}}},\"rows\":{}}}",
        result.metadata.limit,
        result.metadata.offset,
        result.metadata.returned_rows,
        result
            .metadata
            .total_matching_rows
            .map(|count| count.to_string())
            .unwrap_or_else(|| "null".to_string()),
        result.metadata.truncated,
        result.metadata.ordered_by_event_id_ascending,
        rows_json(&result.rows),
    )
}

fn inspection_evidence_json(evidence: Option<&AuditInspectionDiagnosticEvidence>) -> String {
    let Some(evidence) = evidence else {
        return "null".to_string();
    };
    format!(
        "{{\"records_scanned\":{},\"records_matched\":{},\"records_returned\":{},\"filter_applied\":{},\"limit\":{},\"offset\":{},\"truncated\":{}}}",
        evidence.records_scanned,
        evidence.records_matched,
        evidence.records_returned,
        evidence.filter_applied,
        evidence.limit,
        evidence.offset,
        evidence.truncated
    )
}

fn compaction_report_json(report: Option<&DurableAuditCompactionReport>) -> String {
    let Some(report) = report else {
        return "null".to_string();
    };
    format!(
        "{{\"records_scanned\":{},\"records_retained\":{},\"records_expired\":{},\"first_retained_lsn\":{},\"last_retained_lsn\":{},\"retained_checksum_evidence\":{}}}",
        report.records_scanned,
        report.records_retained,
        report.records_expired,
        json_optional_u64(report.first_retained_lsn),
        json_optional_u64(report.last_retained_lsn),
        report.retained_checksum_evidence
    )
}
