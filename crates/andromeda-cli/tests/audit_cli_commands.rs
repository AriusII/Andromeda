#![forbid(unsafe_code)]

use andromeda_cli::cmd::dispatch_command;
use andromeda_core::{RequestId, SessionId};
use andromeda_observe::{
    CertificateIdentity, DurableAuditPrincipalBinding, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, DurableAuditSinkReport, DurableAuditWalSink, EventCorrelation,
    EventEnvelope, EventId, FileDurableAuditWalSink, PendingDurableAuditRecord, Permission,
    SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope, TraceEvent, TraceId, UserPrincipal,
    UserPrincipalKind,
};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn audit_query_accepts_bounded_filters() {
    let args = vec![
        "audit".to_string(),
        "query".to_string(),
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
fn audit_query_rejects_zero_lsn_range() {
    let args = vec![
        "audit".to_string(),
        "query".to_string(),
        "--lsn-range".to_string(),
        "0..0".to_string(),
    ];

    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn audit_query_rejects_zero_limit() {
    let args = vec![
        "audit".to_string(),
        "query".to_string(),
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
        "query".to_string(),
        "--token=super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));

    let args = vec![
        "audit".to_string(),
        "query".to_string(),
        "--family".to_string(),
        "super-secret".to_string(),
    ];

    let err = dispatch_command(&args).unwrap_err();
    assert!(!err.message().contains("super-secret"));
}

#[test]
fn audit_query_rejects_too_large_limit() {
    let args = vec![
        "audit".to_string(),
        "query".to_string(),
        "--limit".to_string(),
        "1001".to_string(),
    ];

    let result = dispatch_command(&args);
    assert!(result.is_err());
}

#[test]
fn audit_query_json_exposes_admin_audit_gate() {
    let output = run_cli([
        "audit",
        "query",
        "--trace-id",
        "42",
        "--principal",
        "user:ops",
        "--limit",
        "5",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);

    assert!(json.trim_start().starts_with('{'));
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.audit.query.v1\"",
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
            "no journal was queried",
        ],
    );
}

#[test]
fn audit_query_diagnostic_json_exposes_replay_evidence() {
    let path = temp_journal_path("query-evidence");
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

    let output = run_cli_vec(vec![
        "audit".to_string(),
        "query".to_string(),
        "--journal".to_string(),
        path.display().to_string(),
        "--principal".to_string(),
        "user:scan-b".to_string(),
        "--family".to_string(),
        "security-audit".to_string(),
        "--limit".to_string(),
        "1".to_string(),
        "--diagnostic-json".to_string(),
    ]);
    assert_success(&output);
    let json = stdout(&output);

    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.audit.query.v1\"",
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
fn audit_compact_diagnostic_json_reports_retention_evidence() {
    let path = temp_journal_path("compact-evidence");
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

    let output = run_cli_vec(vec![
        "audit".to_string(),
        "compact".to_string(),
        "--journal".to_string(),
        path.display().to_string(),
        "--retain-from-lsn".to_string(),
        retained.evidence.record_lsn.to_string(),
        "--preserve-forensic-hold".to_string(),
        "--diagnostic-json".to_string(),
    ]);
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

    let query_after_compaction = run_cli_vec(vec![
        "audit".to_string(),
        "query".to_string(),
        "--journal".to_string(),
        path.display().to_string(),
        "--limit".to_string(),
        "10".to_string(),
        "--diagnostic-json".to_string(),
    ]);
    assert_success(&query_after_compaction);
    assert_contains_all(
        &stdout(&query_after_compaction),
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
    let path = temp_journal_path("verify-corruption");
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

    let output = run_cli_vec(vec![
        "audit".to_string(),
        "verify".to_string(),
        "--journal".to_string(),
        path.display().to_string(),
        "--json".to_string(),
    ]);
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
fn audit_query_does_not_create_or_query_missing_journal() {
    let output = run_cli([
        "audit",
        "query",
        "--journal",
        "target/andromeda-cli/missing-audit-cli-test.log",
        "--json",
    ]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not exist or is not a file"));
}

#[test]
fn audit_help_command_executes() {
    let args = vec!["audit".to_string(), "--help".to_string()];
    let result = dispatch_command(&args);
    assert!(result.is_ok());
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn run_cli_vec(args: Vec<String>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_contains_all(text: &str, expected: &[&str]) {
    for item in expected {
        assert!(text.contains(item), "expected `{item}` in `{text}`");
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
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
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

fn temp_journal_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "andromeda-cli-{test_name}-{}-{nonce}.audit",
        std::process::id()
    ))
}
