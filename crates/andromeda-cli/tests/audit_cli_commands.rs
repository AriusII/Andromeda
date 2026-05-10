#![forbid(unsafe_code)]

use andromeda_audit::{
    AdminOperation, AdminOperationTrace, CertificateIdentity, DurableAuditPrincipalBinding,
    DurableAuditReplayBehavior, DurableAuditRetentionBoundary, DurableAuditSinkReport,
    DurableAuditWalSink, FileDurableAuditWalSink, Permission, SecurityAuditOutcome,
    SecurityAuditTrace, SecurityPolicyVersionEvidence, SurfaceScope, UserPrincipal,
    UserPrincipalKind,
};
use andromeda_cli::dispatch_command;
use andromeda_observability::{EventCorrelation, EventId, TraceId};
use andromeda_observe::{EventEnvelope, PendingDurableAuditRecord, TraceEvent};
use andromeda_test_support::{
    process::{assert_contains_all, assert_success, run_binary, stdout_lossy as stdout},
    workspace::unique_temp_path,
};
use andromeda_types::{RequestId, SessionId};
use std::fs;

#[test]
fn audit_inspect_accepts_bounded_filters() {
    let args = vec![
        "audit".to_string(),
        "inspect".to_string(),
        "--trace-id".to_string(),
        "42".to_string(),
        "--principal".to_string(),
        "user:ops".to_string(),
        "--family".to_string(),
        "security-audit".to_string(),
        "--lsn-range".to_string(),
        "10..20".to_string(),
        "--limit".to_string(),
        "25".to_string(),
        "--offset".to_string(),
        "2".to_string(),
    ];

    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn audit_inspect_rejects_zero_lsn_range() {
    let args = vec![
        "audit".to_string(),
        "inspect".to_string(),
        "--lsn-range".to_string(),
        "0..0".to_string(),
    ];

    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn audit_inspect_accepts_single_lsn_ranges() {
    for args in [
        vec![
            "audit".to_string(),
            "inspect".to_string(),
            "--lsn-range".to_string(),
            "10..10".to_string(),
        ],
        vec![
            "audit".to_string(),
            "inspect".to_string(),
            "--lsn-start".to_string(),
            "10".to_string(),
            "--lsn-end".to_string(),
            "10".to_string(),
        ],
    ] {
        let result = dispatch_command(&args);
        assert!(
            result.is_ok(),
            "single-LSN inclusive range should be accepted: {args:?}"
        );
    }
}

#[test]
fn audit_inspect_rejects_zero_limit() {
    let args = vec![
        "audit".to_string(),
        "inspect".to_string(),
        "--limit".to_string(),
        "0".to_string(),
    ];

    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn audit_parser_errors_do_not_echo_values() {
    let args = vec![
        "audit".to_string(),
        "inspect".to_string(),
        "--token=super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));

    let args = vec![
        "audit".to_string(),
        "inspect".to_string(),
        "--family".to_string(),
        "super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));
}

#[test]
fn audit_principal_filter_rejects_secret_bearing_values() {
    for principal in [
        "token=super-secret",
        "Bearer super-secret",
        "credential=super-secret",
        "x-api-key: super-secret",
        "private_key=super-secret",
    ] {
        let output = run_binary(
            cli_binary(),
            vec![
                "audit".to_string(),
                "inspect".to_string(),
                "--principal".to_string(),
                principal.to_string(),
                "--json".to_string(),
            ],
        );

        assert!(
            !output.status.success(),
            "secret-bearing principal filter must be rejected"
        );
        assert!(!stdout(&output).contains(principal));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(principal));
    }
}

#[test]
fn audit_inspect_rejects_too_large_limit() {
    let args = vec![
        "audit".to_string(),
        "inspect".to_string(),
        "--limit".to_string(),
        "1001".to_string(),
    ];

    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn audit_legacy_query_wording_is_not_a_positive_surface() {
    let args = vec![
        "audit".to_string(),
        "query".to_string(),
        "--limit".to_string(),
        "5".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert_eq!(
        err.message(),
        "unsupported audit subcommand; use `andromeda-cli audit inspect` for durable journal inspection"
    );
}

#[test]
fn audit_inspect_json_exposes_admin_audit_gate() {
    let output = run_binary(
        cli_binary(),
        [
            "audit",
            "inspect",
            "--trace-id",
            "42",
            "--principal",
            "user:ops",
            "--limit",
            "5",
            "--json",
        ],
    );
    assert_success(&output);
    let json = stdout(&output);

    assert!(json.trim_start().starts_with('{'));
    assert_no_application_procedure_or_sql_surface(&json);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.audit.inspection.v1\"",
            "\"diagnostic_only\":true",
            "\"contract_preview\":true",
            "\"durable_backend\":false",
            "\"requires_durable_audit_journal\":true",
            "\"journal_path\":null",
            "\"trace_id\":42",
            "\"principal\":\"user:ops\"",
            "\"limit\":5",
            "\"surface\":\"administration\"",
            "\"required_permission\":\"inspect-plans\"",
            "\"audit_operation\":\"inspect-plans\"",
            "\"audit_required\":true",
            "\"result\":null",
            "no journal was inspected",
        ],
    );
}

#[test]
fn audit_inspect_diagnostic_json_exposes_replay_evidence() {
    let path = unique_temp_path("andromeda-cli-inspection-evidence", ".audit");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    append_security_record(
        &mut sink,
        1,
        "user:scan-a",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let first_match = append_security_record(
        &mut sink,
        2,
        "user:scan-b",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    append_security_record(
        &mut sink,
        3,
        "user:scan-b",
        DurableAuditRetentionBoundary::ForensicHold,
    );
    drop(sink);

    let output = run_binary(
        cli_binary(),
        vec![
            "audit".to_string(),
            "inspect".to_string(),
            "--journal".to_string(),
            path.display().to_string(),
            "--principal".to_string(),
            "user:scan-b".to_string(),
            "--family".to_string(),
            "security-audit".to_string(),
            "--limit".to_string(),
            "1".to_string(),
            "--diagnostic-json".to_string(),
        ],
    );
    assert_success(&output);
    let json = stdout(&output);

    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.audit.inspection.v1\"",
            "\"diagnostic_only\":true",
            "\"durable_backend\":true",
            "\"diagnostic_evidence\":{\"records_scanned\":3,\"records_matched\":2,\"records_returned\":1,\"filter_applied\":true,\"limit\":1,\"offset\":0,\"truncated\":true}",
            "\"returned_rows\":1",
            "\"truncated\":true",
            "\"principal_id\":\"user:scan-b\"",
            &format!("\"record_lsn\":{}", first_match.evidence.record_lsn),
        ],
    );

    let _ = fs::remove_file(path);
}

#[test]
fn audit_inspect_filters_exact_durable_admin_families() {
    let path = unique_temp_path("andromeda-cli-exact-admin-families", ".audit");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    append_admin_record(
        &mut sink,
        31,
        "user:exact-admin",
        AdminOperation::InspectPlans,
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    append_admin_record(
        &mut sink,
        32,
        "user:exact-hadr",
        AdminOperation::ClusterPromote,
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    append_admin_record(
        &mut sink,
        33,
        "user:exact-backup",
        AdminOperation::Backup,
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    append_admin_record(
        &mut sink,
        34,
        "user:exact-restore",
        AdminOperation::Restore,
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    drop(sink);

    for (family_filter, expected_durable_family, expected_principal) in [
        ("admin", "admin-decision", "user:exact-admin"),
        ("hadr", "hadr-decision", "user:exact-hadr"),
        ("backup", "backup-decision", "user:exact-backup"),
        ("restore", "restore-decision", "user:exact-restore"),
    ] {
        let output = run_binary(
            cli_binary(),
            vec![
                "audit".to_string(),
                "inspect".to_string(),
                "--journal".to_string(),
                path.display().to_string(),
                "--family".to_string(),
                family_filter.to_string(),
                "--limit".to_string(),
                "10".to_string(),
                "--include-total-count".to_string(),
                "--diagnostic-json".to_string(),
            ],
        );
        assert_success(&output);
        let json = stdout(&output);
        assert_no_application_procedure_or_sql_surface(&json);

        assert_contains_all(
            &json,
            &[
                "\"returned_rows\":1",
                "\"total_matching_rows\":1",
                &format!("\"durable_audit_family\":\"{expected_durable_family}\""),
                &format!("\"principal_id\":\"{expected_principal}\""),
            ],
        );
        for other_principal in [
            "user:exact-admin",
            "user:exact-hadr",
            "user:exact-backup",
            "user:exact-restore",
        ] {
            if other_principal != expected_principal {
                assert!(
                    !json.contains(other_principal),
                    "exact family filter {family_filter} returned unrelated principal {other_principal}: {json}"
                );
            }
        }
    }

    let broad_output = run_binary(
        cli_binary(),
        vec![
            "audit".to_string(),
            "inspect".to_string(),
            "--journal".to_string(),
            path.display().to_string(),
            "--family".to_string(),
            "admin-audit".to_string(),
            "--limit".to_string(),
            "10".to_string(),
            "--include-total-count".to_string(),
            "--diagnostic-json".to_string(),
        ],
    );
    assert_success(&broad_output);
    let broad_json = stdout(&broad_output);
    assert_no_application_procedure_or_sql_surface(&broad_json);
    assert_contains_all(
        &broad_json,
        &[
            "\"returned_rows\":4",
            "\"total_matching_rows\":4",
            "\"durable_audit_family\":\"admin-decision\"",
            "\"durable_audit_family\":\"hadr-decision\"",
            "\"durable_audit_family\":\"backup-decision\"",
            "\"durable_audit_family\":\"restore-decision\"",
        ],
    );

    let _ = fs::remove_file(path);
}

#[test]
fn audit_compact_diagnostic_json_reports_retention_evidence() {
    let path = unique_temp_path("andromeda-cli-compact-evidence", ".audit");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let expired = append_security_record(
        &mut sink,
        10,
        "user:expired",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let retained = append_security_record(
        &mut sink,
        11,
        "user:retained",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let forensic_hold = append_security_record(
        &mut sink,
        12,
        "user:forensic",
        DurableAuditRetentionBoundary::ForensicHold,
    );
    drop(sink);

    let output = run_binary(
        cli_binary(),
        vec![
            "audit".to_string(),
            "compact".to_string(),
            "--journal".to_string(),
            path.display().to_string(),
            "--retain-from-lsn".to_string(),
            retained.evidence.record_lsn.to_string(),
            "--preserve-forensic-hold".to_string(),
            "--diagnostic-json".to_string(),
        ],
    );
    assert_success(&output);
    let json = stdout(&output);

    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.audit.compact.v1\"",
            "\"diagnostic_only\":true",
            "\"durable_backend\":true",
            "\"records_scanned\":3",
            "\"records_retained\":3",
            "\"records_expired\":0",
            &format!("\"first_retained_lsn\":{}", expired.evidence.record_lsn),
            &format!(
                "\"last_retained_lsn\":{}",
                forensic_hold.evidence.record_lsn
            ),
            "\"retained_checksum_evidence\":",
            "durable audit journal compaction completed",
        ],
    );

    let inspection_after_compaction = run_binary(
        cli_binary(),
        vec![
            "audit".to_string(),
            "inspect".to_string(),
            "--journal".to_string(),
            path.display().to_string(),
            "--limit".to_string(),
            "10".to_string(),
            "--diagnostic-json".to_string(),
        ],
    );
    assert_success(&inspection_after_compaction);
    assert_contains_all(
        &stdout(&inspection_after_compaction),
        &[
            "\"diagnostic_evidence\":{\"records_scanned\":3,\"records_matched\":3,\"records_returned\":3,\"filter_applied\":false,\"limit\":10,\"offset\":0,\"truncated\":false}",
            "\"principal_id\":\"user:expired\"",
            "\"principal_id\":\"user:retained\"",
            "\"principal_id\":\"user:forensic\"",
        ],
    );

    let _ = fs::remove_file(path);
}

#[test]
fn audit_verify_detects_checksum_corruption() {
    let path = unique_temp_path("andromeda-cli-verify-corruption", ".audit");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    append_security_record(
        &mut sink,
        21,
        "user:verify",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    drop(sink);

    let mut journal = fs::read_to_string(&path).expect("journal is readable");
    journal = journal.replacen("SecurityDecision", "SecurityDecisioN", 1);
    fs::write(&path, journal).expect("corruption is written");

    let output = run_binary(
        cli_binary(),
        vec![
            "audit".to_string(),
            "verify".to_string(),
            "--journal".to_string(),
            path.display().to_string(),
            "--json".to_string(),
        ],
    );
    assert!(
        !output.status.success(),
        "audit verify should fail closed on checksum corruption"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("checksum mismatch") || stderr.contains("corruption"),
        "unexpected stderr: {stderr}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn audit_inspect_does_not_create_or_inspect_missing_journal() {
    let output = run_binary(
        cli_binary(),
        [
            "audit",
            "inspect",
            "--journal",
            "target/andromeda-cli/missing-audit-cli-test.log",
            "--json",
        ],
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not exist or is not a file"));
}

#[test]
fn audit_help_command_executes() {
    let args = vec!["audit".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

#[test]
fn audit_help_uses_inspection_wording() {
    let output = run_binary(cli_binary(), ["audit", "--help"]);

    assert_success(&output);
    let help = stdout(&output);
    assert_contains_all(
        &help,
        &[
            "Inspect durable audit journal replay through the trace inspection contract",
            "Filter by trace family",
            "Emit replay evidence fields in diagnostic JSON",
        ],
    );
    assert!(!help.contains("  query"));
    assert!(!help.contains("Query a durable audit journal"));
    assert!(!help.contains("Filter by query family"));
}

fn cli_binary() -> &'static str {
    env!("CARGO_BIN_EXE_andromeda-cli")
}

fn assert_no_application_procedure_or_sql_surface(text: &str) {
    for forbidden in [
        "\"surface\":\"application\"",
        "\"required_permission\":\"execute-procedure\"",
        "\"permission\":\"execute-procedure\"",
        "\"family\":\"procedure-invocation\"",
        "\"procedure_id\"",
        "\"procedure\"",
    ] {
        assert!(
            !text.contains(forbidden),
            "audit admin diagnostic output exposed application procedure token `{forbidden}` in `{text}`"
        );
    }

    let lower = text.to_ascii_lowercase();
    for forbidden in [
        "\"sql\"",
        "\"query\"",
        "--sql",
        "select ",
        "insert ",
        "update ",
        "delete ",
        " from ",
        " where ",
    ] {
        assert!(
            !lower.contains(forbidden),
            "audit admin diagnostic output exposed ad hoc SQL token `{forbidden}` in `{text}`"
        );
    }
}

fn request_correlation(event_id: u128) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn admin_envelope(event_id: u128, principal_id: &str, operation: AdminOperation) -> EventEnvelope {
    let surface = surface_for_admin_operation(operation);
    let certificate = CertificateIdentity::new(
        format!("sha256:cli-audit-{event_id}"),
        "CN=cli-audit",
        surface,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new(principal_id, UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = AdminOperationTrace::new(
        TraceId::new(event_id + 1_000),
        surface,
        certificate,
        principal,
        operation,
        operation.required_permission(),
        true,
        format!("cli audit admin event {event_id}"),
    )
    .expect("admin operation trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::AdminOperation(trace),
    )
    .expect("admin operation envelope is valid")
}

fn surface_for_admin_operation(operation: AdminOperation) -> SurfaceScope {
    match operation {
        AdminOperation::ClusterPromote
        | AdminOperation::FenceNode
        | AdminOperation::UpdateClusterManifest => SurfaceScope::Cluster,
        AdminOperation::Backup | AdminOperation::Restore | AdminOperation::ForensicStart => {
            SurfaceScope::BackupAgent
        },
        AdminOperation::DebugProcedure
        | AdminOperation::ReadProcedureStore
        | AdminOperation::InspectPlans
        | AdminOperation::ManageSecurity
        | AdminOperation::RotateCertificate
        | AdminOperation::RevokeCertificateIdentity => SurfaceScope::Administration,
    }
}

fn security_envelope(event_id: u128, trace_id: u128, principal_id: &str) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:cli-audit-{event_id}"),
        "CN=cli-audit",
        SurfaceScope::Administration,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new(principal_id, UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Administration,
        certificate,
        principal,
        Permission::InspectPlans,
        SecurityAuditOutcome::Allowed,
        format!("cli audit event {event_id}"),
    )
    .expect("security audit trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit envelope is valid")
}

fn principal_binding(event_id: u128, principal_id: &str) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: principal_id.to_string(),
        certificate_fingerprint: Some(format!("sha256:cli-audit-{event_id}")),
        surface: Some(SurfaceScope::Administration),
        permission: Some(Permission::InspectPlans),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
}

fn admin_principal_binding(
    event_id: u128,
    principal_id: &str,
    operation: AdminOperation,
) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: principal_id.to_string(),
        certificate_fingerprint: Some(format!("sha256:cli-audit-{event_id}")),
        surface: Some(surface_for_admin_operation(operation)),
        permission: Some(operation.required_permission()),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
}

fn append_admin_record(
    sink: &mut FileDurableAuditWalSink,
    event_id: u128,
    principal_id: &str,
    operation: AdminOperation,
    retention: DurableAuditRetentionBoundary,
) -> DurableAuditSinkReport {
    let record = PendingDurableAuditRecord::new(
        event_id as u64,
        admin_principal_binding(event_id, principal_id, operation),
        retention,
        DurableAuditReplayBehavior::ForensicOnly,
        admin_envelope(event_id, principal_id, operation),
    )
    .expect("durable admin audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append is flushed before success")
}

fn append_security_record(
    sink: &mut FileDurableAuditWalSink,
    event_id: u128,
    principal_id: &str,
    retention: DurableAuditRetentionBoundary,
) -> DurableAuditSinkReport {
    let record = PendingDurableAuditRecord::new(
        event_id as u64,
        principal_binding(event_id, principal_id),
        retention,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(event_id, event_id + 1_000, principal_id),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append is flushed before success")
}
