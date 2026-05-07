mod output;
mod parsing;
mod sensitive;

use std::path::PathBuf;

use andromeda_core::AndromedaResult;
use andromeda_observe::{
    DurableAuditCompactionReport, DurableAuditEventFamily, DurableAuditReplayEvidence,
    DurableAuditReplayLsnRange, DurableAuditReplayQuery, DurableAuditReplayWindow,
    DurableAuditRetentionPolicy, DurableAuditTraceQueryResult, DurableAuditTraceQuerySource,
    FileDurableAuditWalSink, TraceEventFamily, TraceQueryLsnRange, TraceQuerySpec,
};

use crate::error::cli_error;

use self::output::{
    print_audit_compact_result, print_audit_help, print_audit_inspection_result,
    print_audit_verify_result,
};
use self::parsing::{
    parse_audit_compact_options, parse_audit_inspection_options, parse_audit_verify_options,
};
use self::sensitive::redact_sensitive_cli_evidence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditInspectionOptions {
    pub(super) spec: TraceQuerySpec,
    pub(super) json_output: bool,
    pub(super) diagnostic_json: bool,
    pub(super) journal_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditCompactOptions {
    pub(super) journal_path: Option<PathBuf>,
    pub(super) retain_from_lsn: u64,
    pub(super) preserve_forensic_hold: bool,
    pub(super) diagnostic_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditVerifyOptions {
    pub(super) journal_path: Option<PathBuf>,
    pub(super) json_output: bool,
    pub(super) diagnostic_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditInspectionReport {
    pub(super) schema: &'static str,
    pub(super) contract_preview: bool,
    pub(super) durable_backend: bool,
    pub(super) requires_durable_audit_journal: bool,
    pub(super) journal_path: Option<String>,
    pub(super) spec: TraceQuerySpec,
    pub(super) result: Option<DurableAuditTraceQueryResult>,
    pub(super) diagnostic_evidence: Option<AuditInspectionDiagnosticEvidence>,
    pub(super) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AuditInspectionDiagnosticEvidence {
    pub(super) records_scanned: usize,
    pub(super) records_matched: usize,
    pub(super) records_returned: usize,
    pub(super) filter_applied: bool,
    pub(super) limit: usize,
    pub(super) offset: usize,
    pub(super) truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditCompactReport {
    pub(super) schema: &'static str,
    pub(super) contract_preview: bool,
    pub(super) durable_backend: bool,
    pub(super) requires_durable_audit_journal: bool,
    pub(super) journal_path: Option<String>,
    pub(super) retain_from_lsn: u64,
    pub(super) preserve_forensic_hold: bool,
    pub(super) report: Option<DurableAuditCompactionReport>,
    pub(super) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditVerifyReport {
    pub(super) schema: &'static str,
    pub(super) durable_backend: bool,
    pub(super) requires_durable_audit_journal: bool,
    pub(super) journal_path: String,
    pub(super) records_scanned: usize,
    pub(super) records_returned: usize,
    pub(super) first_returned_lsn: Option<u64>,
    pub(super) last_returned_lsn: Option<u64>,
    pub(super) message: String,
}

pub fn run_audit_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("inspect") => run_audit_inspection(&args[1..]),
        Some("query") => Err(cli_error(
            "unsupported audit subcommand; use `andromeda-cli audit inspect` for durable journal inspection",
        )),
        Some("compact") => run_audit_compact(&args[1..]),
        Some("verify") => run_audit_verify(&args[1..]),
        Some("-h" | "--help" | "help") | None => {
            print_audit_help();
            Ok(())
        }
        Some(_) => Err(cli_error(
            "unknown audit subcommand; run `andromeda-cli audit --help`",
        )),
    }
}

fn run_audit_inspection(args: &[String]) -> AndromedaResult<()> {
    let options = parse_audit_inspection_options(args)?;
    let report = build_audit_inspection_report(&options)?;
    print_audit_inspection_result(&report, options.json_output, options.diagnostic_json);
    Ok(())
}

fn run_audit_compact(args: &[String]) -> AndromedaResult<()> {
    let options = parse_audit_compact_options(args)?;
    let report = build_audit_compact_report(&options)?;
    print_audit_compact_result(&report, options.diagnostic_json);
    Ok(())
}

fn run_audit_verify(args: &[String]) -> AndromedaResult<()> {
    let options = parse_audit_verify_options(args)?;
    let report = build_audit_verify_report(&options)?;
    print_audit_verify_result(&report, options.json_output || options.diagnostic_json);
    Ok(())
}

fn build_audit_inspection_report(
    options: &AuditInspectionOptions,
) -> AndromedaResult<AuditInspectionReport> {
    options.spec.validate()?;

    let Some(journal_path) = &options.journal_path else {
        return Ok(AuditInspectionReport {
            schema: "andromeda.cli.audit.inspection.v1",
            contract_preview: true,
            durable_backend: false,
            requires_durable_audit_journal: true,
            journal_path: None,
            spec: options.spec.clone(),
            result: None,
            diagnostic_evidence: None,
            message: "contract preview: provide --journal <path> to replay a durable audit journal; no journal was inspected".to_string(),
        });
    };

    if !journal_path.is_file() {
        return Err(cli_error(format!(
            "audit inspect journal `{}` does not exist or is not a file",
            audit_path_for_output(journal_path)
        )));
    }

    let sink = FileDurableAuditWalSink::open(journal_path).map_err(|error| {
        cli_error(format!(
            "failed to open durable audit journal `{}`: {}",
            audit_path_for_output(journal_path),
            error.reason
        ))
    })?;
    let replay = sink
        .query_with_evidence(
            &durable_replay_filter_from_trace_spec(&options.spec),
            DurableAuditReplayWindow::ALL,
        )
        .map_err(|error| {
            cli_error(format!(
                "failed to replay durable audit journal `{}`: {}",
                audit_path_for_output(journal_path),
                error.reason
            ))
        })?;
    let source = DurableAuditTraceQuerySource::new(&replay.records);
    let result = source.query(&options.spec)?;
    let diagnostic_evidence = options
        .diagnostic_json
        .then(|| build_inspection_diagnostic_evidence(&options.spec, replay.evidence, &source))
        .transpose()?;

    Ok(AuditInspectionReport {
        schema: "andromeda.cli.audit.inspection.v1",
        contract_preview: false,
        durable_backend: true,
        requires_durable_audit_journal: true,
        journal_path: Some(audit_path_for_output(journal_path)),
        spec: options.spec.clone(),
        result: Some(result),
        diagnostic_evidence,
        message: "durable audit journal replay inspection completed".to_string(),
    })
}

fn build_audit_compact_report(
    options: &AuditCompactOptions,
) -> AndromedaResult<AuditCompactReport> {
    if options.retain_from_lsn == 0 {
        return Err(cli_error(
            "audit compact --retain-from-lsn must be a non-zero LSN",
        ));
    }

    let Some(journal_path) = &options.journal_path else {
        return Ok(AuditCompactReport {
            schema: "andromeda.cli.audit.compact.v1",
            contract_preview: true,
            durable_backend: false,
            requires_durable_audit_journal: true,
            journal_path: None,
            retain_from_lsn: options.retain_from_lsn,
            preserve_forensic_hold: options.preserve_forensic_hold,
            report: None,
            message: "contract preview: provide --journal <path> to compact a durable audit journal; no journal was rewritten".to_string(),
        });
    };

    if !journal_path.is_file() {
        return Err(cli_error(format!(
            "audit compact journal `{}` does not exist or is not a file",
            audit_path_for_output(journal_path)
        )));
    }

    let sink = FileDurableAuditWalSink::open(journal_path).map_err(|error| {
        cli_error(format!(
            "failed to open durable audit journal `{}`: {}",
            audit_path_for_output(journal_path),
            error.reason
        ))
    })?;
    let policy =
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(options.retain_from_lsn)
            .with_forensic_hold_preserved(options.preserve_forensic_hold);
    let report = sink.compact(&policy).map_err(|error| {
        cli_error(format!(
            "failed to compact durable audit journal `{}`: {}",
            audit_path_for_output(journal_path),
            error.reason
        ))
    })?;

    Ok(AuditCompactReport {
        schema: "andromeda.cli.audit.compact.v1",
        contract_preview: false,
        durable_backend: true,
        requires_durable_audit_journal: true,
        journal_path: Some(audit_path_for_output(journal_path)),
        retain_from_lsn: options.retain_from_lsn,
        preserve_forensic_hold: options.preserve_forensic_hold,
        report: Some(report),
        message: "durable audit journal compaction completed".to_string(),
    })
}

fn build_audit_verify_report(options: &AuditVerifyOptions) -> AndromedaResult<AuditVerifyReport> {
    let Some(journal_path) = &options.journal_path else {
        return Err(cli_error(
            "audit verify requires --journal <path> to avoid creating a missing journal",
        ));
    };

    if !journal_path.is_file() {
        return Err(cli_error(format!(
            "audit verify journal `{}` does not exist or is not a file",
            audit_path_for_output(journal_path)
        )));
    }

    let sink = FileDurableAuditWalSink::open(journal_path).map_err(|error| {
        cli_error(format!(
            "failed to verify durable audit journal `{}`: {}",
            audit_path_for_output(journal_path),
            error.reason
        ))
    })?;
    let replay = sink
        .query_with_evidence(
            &DurableAuditReplayQuery::all(),
            DurableAuditReplayWindow::ALL,
        )
        .map_err(|error| {
            cli_error(format!(
                "failed to verify durable audit journal `{}`: {}",
                audit_path_for_output(journal_path),
                error.reason
            ))
        })?;

    Ok(AuditVerifyReport {
        schema: "andromeda.cli.audit.verify.v1",
        durable_backend: true,
        requires_durable_audit_journal: true,
        journal_path: audit_path_for_output(journal_path),
        records_scanned: replay.evidence.records_scanned,
        records_returned: replay.evidence.records_returned,
        first_returned_lsn: replay.evidence.first_returned_lsn,
        last_returned_lsn: replay.evidence.last_returned_lsn,
        message: "durable audit journal checksum verification completed".to_string(),
    })
}

fn build_inspection_diagnostic_evidence(
    spec: &TraceQuerySpec,
    replay_evidence: DurableAuditReplayEvidence,
    source: &DurableAuditTraceQuerySource<'_>,
) -> AndromedaResult<AuditInspectionDiagnosticEvidence> {
    let mut diagnostic_spec = spec.clone();
    diagnostic_spec.include_total_count = true;
    let diagnostic_result = source.query(&diagnostic_spec)?;
    Ok(AuditInspectionDiagnosticEvidence {
        records_scanned: replay_evidence.records_scanned,
        records_matched: diagnostic_result
            .metadata
            .total_matching_rows
            .unwrap_or(diagnostic_result.metadata.returned_rows),
        records_returned: diagnostic_result.metadata.returned_rows,
        filter_applied: !spec.filter.is_unbounded(),
        limit: spec.limit,
        offset: spec.offset,
        truncated: diagnostic_result.metadata.truncated,
    })
}

fn durable_replay_filter_from_trace_spec(spec: &TraceQuerySpec) -> DurableAuditReplayQuery {
    DurableAuditReplayQuery {
        family: spec
            .filter
            .family
            .and_then(durable_family_for_exact_trace_family),
        trace_id: spec.filter.trace_id,
        principal_id: spec.filter.principal.clone(),
        lsn_range: spec.filter.lsn_range.map(durable_lsn_range),
    }
}

fn durable_lsn_range(range: TraceQueryLsnRange) -> DurableAuditReplayLsnRange {
    DurableAuditReplayLsnRange::new(range.start_lsn, range.end_lsn)
}

fn durable_family_for_exact_trace_family(
    family: TraceEventFamily,
) -> Option<DurableAuditEventFamily> {
    match family {
        TraceEventFamily::SecurityAudit => Some(DurableAuditEventFamily::SecurityDecision),
        TraceEventFamily::Protocol => Some(DurableAuditEventFamily::AdmissionDecision),
        TraceEventFamily::ManifestCatalog => Some(DurableAuditEventFamily::CatalogDecision),
        _ => None,
    }
}

fn audit_path_for_output(path: &std::path::Path) -> String {
    let display = path.display().to_string();
    redact_sensitive_cli_evidence(&display).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observe::{TRACE_QUERY_MAX_LIMIT, TraceId};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn audit_inspect_defaults_to_contract_preview_without_journal() {
        let options = parse_audit_inspection_options(&strings(&[
            "--trace-id",
            "42",
            "--principal",
            "user:ops",
            "--limit",
            "10",
        ]))
        .unwrap();
        let report = build_audit_inspection_report(&options).unwrap();

        assert!(report.contract_preview);
        assert!(!report.durable_backend);
        assert!(report.requires_durable_audit_journal);
        assert_eq!(report.spec.filter.trace_id, Some(TraceId::new(42)));
        assert_eq!(report.spec.limit, 10);
        assert!(report.result.is_none());
        assert!(report.diagnostic_evidence.is_none());
    }

    #[test]
    fn audit_inspect_rejects_unbounded_limits() {
        let zero = parse_audit_inspection_options(&strings(&["--limit", "0"]));
        assert!(zero.is_err());

        let too_large = parse_audit_inspection_options(&strings(&[
            "--limit",
            &(TRACE_QUERY_MAX_LIMIT + 1).to_string(),
        ]));
        assert!(too_large.is_err());
    }

    #[test]
    fn audit_inspect_rejects_missing_journal_file() {
        let options = parse_audit_inspection_options(&strings(&[
            "--journal",
            "target/andromeda-cli/missing-audit-journal.log",
        ]))
        .unwrap();
        let error = build_audit_inspection_report(&options).unwrap_err();
        assert!(error.message().contains("does not exist or is not a file"));
    }

    #[test]
    fn audit_compact_defaults_to_contract_preview_without_journal() {
        let options = parse_audit_compact_options(&strings(&["--retain-from-lsn", "42"])).unwrap();
        let report = build_audit_compact_report(&options).unwrap();

        assert!(report.contract_preview);
        assert!(!report.durable_backend);
        assert_eq!(report.retain_from_lsn, 42);
        assert!(report.preserve_forensic_hold);
        assert!(report.report.is_none());
    }
}
