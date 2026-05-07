use andromeda_observe::{
    AdminOperation, DurableAuditCompactionReport, DurableAuditEventFamily,
    DurableAuditReplayBehavior, DurableAuditRetentionBoundary, DurableAuditTraceQueryResult,
    DurableAuditTraceQueryRow, Permission, SurfaceScope, TraceEventFamily, TraceQueryFilter,
    TraceQueryLsnRange, TraceQueryPermissionMatrix, TraceQuerySpec,
};

use crate::diagnostic_json::{json_option_u64, json_string};

use super::{
    AuditCompactReport, AuditInspectionDiagnosticEvidence, AuditInspectionReport,
    AuditVerifyReport, sensitive::redact_sensitive_cli_evidence,
};

pub(super) fn print_audit_help() {
    println!("Andromeda audit administration commands");
    println!();
    println!("USAGE: andromeda-cli audit <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!(
        "  inspect  Inspect durable audit journal replay through the trace inspection contract"
    );
    println!("  compact  Rewrite a durable audit journal through the retention contract");
    println!("  verify   Verify durable audit journal checksums without creating missing files");
    println!();
    println!("INSPECT OPTIONS:");
    println!("  --journal <path>          Durable audit journal file to replay");
    println!("  --trace-id <u128>         Filter by trace id");
    println!("  --principal <id>          Filter by durable principal id");
    println!("  --family <family>         Filter by trace family");
    println!("  --lsn-range <start..end>  Inclusive LSN filter range");
    println!("  --lsn-start <lsn>         Inclusive LSN filter start");
    println!("  --lsn-end <lsn>           Inclusive LSN filter end");
    println!("  --limit <n>               Max rows; 1..=TRACE_QUERY_MAX_LIMIT");
    println!("  --offset <n>              Rows to skip after filtering");
    println!("  --include-total-count     Include full matching count");
    println!("  --json                    Emit machine-readable JSON output");
    println!("  --diagnostic-json         Emit replay evidence fields in diagnostic JSON");
    println!();
    println!("COMPACT OPTIONS:");
    println!("  --journal <path>          Durable audit journal file to rewrite");
    println!("  --retain-from-lsn <lsn>   Keep records at or after this record LSN");
    println!("  --preserve-forensic-hold  Preserve forensic hold records below the LSN floor");
    println!("  --diagnostic-json         Emit compaction report as diagnostic JSON");
    println!();
    println!("VERIFY OPTIONS:");
    println!("  --journal <path>          Durable audit journal file to verify");
    println!("  --json                    Emit verification report as JSON");
    println!("  --diagnostic-json         Emit verification report as diagnostic JSON");
    println!("  -h, --help                Show this help message");
}

pub(super) fn print_audit_inspection_result(
    report: &AuditInspectionReport,
    json_output: bool,
    diagnostic_json: bool,
) {
    if json_output {
        print_audit_inspection_json(report, diagnostic_json);
    } else {
        print_audit_inspection_human(report);
    }
}

pub(super) fn print_audit_compact_result(report: &AuditCompactReport, diagnostic_json: bool) {
    if diagnostic_json {
        print_audit_compact_json(report);
    } else {
        print_audit_compact_human(report);
    }
}

pub(super) fn print_audit_verify_result(report: &AuditVerifyReport, json_output: bool) {
    if json_output {
        print_audit_verify_json(report);
    } else {
        print_audit_verify_human(report);
    }
}

fn print_audit_inspection_human(report: &AuditInspectionReport) {
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
    println!(
        "Permission: {} on {} (audit required: {})",
        permission_str(report.spec_permission_matrix().required_permission),
        surface_str(report.spec_permission_matrix().surface),
        report.spec_permission_matrix().audit_required
    );
    println!("Limit: {}", report.spec.limit);
    println!("Offset: {}", report.spec.offset);
    print_filters_human(&report.spec.filter);
    if let Some(result) = &report.result {
        println!("Returned Rows: {}", result.metadata.returned_rows);
        println!("Truncated: {}", result.metadata.truncated);
        for row in &result.rows {
            println!(
                "- event_id={} trace_id={} family={} record_lsn={} principal={} event_kind={}",
                row.event_id.get(),
                row.trace_id.get(),
                trace_family_str(row.family),
                row.record_lsn,
                audit_output_text(&row.principal_id),
                audit_output_text(&row.event_kind)
            );
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

fn print_audit_inspection_json(report: &AuditInspectionReport, _diagnostic_json: bool) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"contract_preview\":{},\"durable_backend\":{},\"requires_durable_audit_journal\":{},\"journal_path\":{},\"inspection\":{},\"permission_matrix\":{},\"diagnostic_evidence\":{},\"result\":{},\"message\":{}}}",
        json_string(report.schema),
        report.contract_preview,
        report.durable_backend,
        report.requires_durable_audit_journal,
        json_option_audit_string(report.journal_path.as_deref()),
        inspection_spec_json(&report.spec),
        permission_matrix_json(report.spec_permission_matrix()),
        inspection_evidence_json(report.diagnostic_evidence.as_ref()),
        result_json(report.result.as_ref()),
        json_audit_string(&report.message),
    );
}

fn print_audit_compact_human(report: &AuditCompactReport) {
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

fn print_audit_compact_json(report: &AuditCompactReport) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"contract_preview\":{},\"durable_backend\":{},\"requires_durable_audit_journal\":{},\"journal_path\":{},\"policy\":{{\"retain_from_lsn\":{},\"preserve_forensic_hold\":{}}},\"report\":{},\"message\":{}}}",
        json_string(report.schema),
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

fn print_audit_verify_human(report: &AuditVerifyReport) {
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

fn print_audit_verify_json(report: &AuditVerifyReport) {
    println!(
        "{{\"schema\":{},\"diagnostic_only\":true,\"durable_backend\":{},\"requires_durable_audit_journal\":{},\"journal_path\":{},\"records_scanned\":{},\"records_returned\":{},\"first_returned_lsn\":{},\"last_returned_lsn\":{},\"message\":{}}}",
        json_string(report.schema),
        report.durable_backend,
        report.requires_durable_audit_journal,
        json_audit_string(&report.journal_path),
        report.records_scanned,
        report.records_returned,
        json_option_u64(report.first_returned_lsn),
        json_option_u64(report.last_returned_lsn),
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
            .map(|family| json_string(trace_family_str(family)))
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
        json_string(surface_str(matrix.surface)),
        json_string(permission_str(matrix.required_permission)),
        json_string(admin_operation_str(matrix.audit_operation)),
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
        json_option_u64(report.first_retained_lsn),
        json_option_u64(report.last_retained_lsn),
        report.retained_checksum_evidence
    )
}

fn option_u64_human(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn rows_json(rows: &[DurableAuditTraceQueryRow]) -> String {
    let entries = rows.iter().map(row_json).collect::<Vec<_>>().join(",");
    format!("[{}]", entries)
}

fn row_json(row: &DurableAuditTraceQueryRow) -> String {
    format!(
        "{{\"event_id\":{},\"trace_id\":{},\"family\":{},\"durable_audit_family\":{},\"sequence_number\":{},\"record_lsn\":{},\"durable_lsn\":{},\"checksum\":{},\"replay_behavior\":{},\"retention\":{},\"principal_id\":{},\"certificate_fingerprint\":{},\"surface\":{},\"permission\":{},\"request_id\":{},\"session_id\":{},\"event_kind\":{}}}",
        row.event_id.get(),
        row.trace_id.get(),
        json_string(trace_family_str(row.family)),
        json_string(durable_family_str(row.durable_audit_family)),
        row.sequence_number,
        row.record_lsn,
        row.durable_lsn,
        row.checksum,
        json_string(replay_behavior_str(row.replay_behavior)),
        json_string(retention_str(row.retention)),
        json_option_audit_string(Some(&row.principal_id)),
        json_option_audit_string(row.certificate_fingerprint.as_deref()),
        row.surface
            .map(|surface| json_string(surface_str(surface)))
            .unwrap_or_else(|| "null".to_string()),
        row.permission
            .map(|permission| json_string(permission_str(permission)))
            .unwrap_or_else(|| "null".to_string()),
        row.request_id
            .map(|request_id| request_id.get().to_string())
            .unwrap_or_else(|| "null".to_string()),
        row.session_id
            .map(|session_id| session_id.get().to_string())
            .unwrap_or_else(|| "null".to_string()),
        json_audit_string(&row.event_kind),
    )
}

fn audit_output_text(value: &str) -> &str {
    redact_sensitive_cli_evidence(value)
}

fn json_audit_string(value: &str) -> String {
    json_string(audit_output_text(value))
}

fn json_option_audit_string(value: Option<&str>) -> String {
    value
        .map(json_audit_string)
        .unwrap_or_else(|| "null".to_string())
}

trait AuditInspectionReportPermission {
    fn spec_permission_matrix(&self) -> TraceQueryPermissionMatrix;
}

impl AuditInspectionReportPermission for AuditInspectionReport {
    fn spec_permission_matrix(&self) -> TraceQueryPermissionMatrix {
        TraceQueryPermissionMatrix::V1_ADMIN
    }
}

fn trace_family_str(family: TraceEventFamily) -> &'static str {
    match family {
        TraceEventFamily::Decision => "decision",
        TraceEventFamily::ProcedureInvocation => "procedure-invocation",
        TraceEventFamily::Wal => "wal",
        TraceEventFamily::Recovery => "recovery",
        TraceEventFamily::ManifestCatalog => "manifest-catalog",
        TraceEventFamily::Protocol => "protocol",
        TraceEventFamily::SecurityAudit => "security-audit",
        TraceEventFamily::AdminAudit => "admin-audit",
        TraceEventFamily::Resource => "resource",
        TraceEventFamily::Io => "io",
        TraceEventFamily::Gpu => "gpu",
        TraceEventFamily::Transaction => "transaction",
    }
}

fn durable_family_str(family: DurableAuditEventFamily) -> &'static str {
    match family {
        DurableAuditEventFamily::SecurityDecision => "security-decision",
        DurableAuditEventFamily::AdminDecision => "admin-decision",
        DurableAuditEventFamily::AdmissionDecision => "admission-decision",
        DurableAuditEventFamily::CatalogDecision => "catalog-decision",
        DurableAuditEventFamily::HadrDecision => "hadr-decision",
        DurableAuditEventFamily::BackupDecision => "backup-decision",
        DurableAuditEventFamily::RestoreDecision => "restore-decision",
        DurableAuditEventFamily::ForensicDecision => "forensic-decision",
        DurableAuditEventFamily::RecoveryDecision => "recovery-decision",
        DurableAuditEventFamily::GenericAudit => "generic-audit",
    }
}

fn replay_behavior_str(value: DurableAuditReplayBehavior) -> &'static str {
    match value {
        DurableAuditReplayBehavior::ForensicOnly => "forensic-only",
        DurableAuditReplayBehavior::RebuildDecisionIndex => "rebuild-decision-index",
        DurableAuditReplayBehavior::CorruptionBoundary => "corruption-boundary",
    }
}

fn retention_str(value: DurableAuditRetentionBoundary) -> &'static str {
    match value {
        DurableAuditRetentionBoundary::WalSegment => "wal-segment",
        DurableAuditRetentionBoundary::CatalogVersion => "catalog-version",
        DurableAuditRetentionBoundary::SecurityPolicy => "security-policy",
        DurableAuditRetentionBoundary::ForensicHold => "forensic-hold",
    }
}

fn surface_str(surface: SurfaceScope) -> &'static str {
    match surface {
        SurfaceScope::Application => "application",
        SurfaceScope::Administration => "administration",
        SurfaceScope::Cluster => "cluster",
        SurfaceScope::BackupAgent => "backup-agent",
        SurfaceScope::MonitoringAgent => "monitoring-agent",
    }
}

fn permission_str(permission: Permission) -> &'static str {
    match permission {
        Permission::ExecuteProcedure => "execute-procedure",
        Permission::ReadContract => "read-contract",
        Permission::CreateTable => "create-table",
        Permission::CreateMap => "create-map",
        Permission::CreateProcedure => "create-procedure",
        Permission::ImportDefinitionBatch => "import-definition-batch",
        Permission::DebugProcedure => "debug-procedure",
        Permission::ReadProcedureStore => "read-procedure-store",
        Permission::InspectPlans => "inspect-plans",
        Permission::ManageSecurity => "manage-security",
        Permission::RotateCertificate => "rotate-certificate",
        Permission::RevokeCertificateIdentity => "revoke-certificate-identity",
        Permission::Backup => "backup",
        Permission::Restore => "restore",
        Permission::ForensicStart => "forensic-start",
        Permission::ClusterPromote => "cluster-promote",
        Permission::FenceNode => "fence-node",
        Permission::UpdateClusterManifest => "update-cluster-manifest",
    }
}

fn admin_operation_str(operation: AdminOperation) -> &'static str {
    match operation {
        AdminOperation::DebugProcedure => "debug-procedure",
        AdminOperation::ReadProcedureStore => "read-procedure-store",
        AdminOperation::InspectPlans => "inspect-plans",
        AdminOperation::ManageSecurity => "manage-security",
        AdminOperation::RotateCertificate => "rotate-certificate",
        AdminOperation::RevokeCertificateIdentity => "revoke-certificate-identity",
        AdminOperation::Backup => "backup",
        AdminOperation::Restore => "restore",
        AdminOperation::ForensicStart => "forensic-start",
        AdminOperation::ClusterPromote => "cluster-promote",
        AdminOperation::FenceNode => "fence-node",
        AdminOperation::UpdateClusterManifest => "update-cluster-manifest",
    }
}
