use std::path::PathBuf;

use andromeda_error::AndromedaResult;
use andromeda_observe::{
    DurableAuditEventFamily, TRACE_QUERY_MAX_LIMIT, TraceEventFamily, TraceId, TraceQueryFilter,
    TraceQueryLsnRange, TraceQuerySpec,
};

use crate::diagnostic_json::{DIAGNOSTIC_JSON_FLAG, JSON_FLAG};
use crate::error::cli_error;

use super::sensitive::contains_sensitive_cli_evidence;
use super::{AuditCompactOptions, AuditInspectionOptions, AuditVerifyOptions};

pub(super) fn parse_audit_inspection_options(
    args: &[String],
) -> AndromedaResult<AuditInspectionOptions> {
    let mut json_output = false;
    let mut diagnostic_json = false;
    let mut journal_path = None;
    let mut filter = TraceQueryFilter::default();
    let mut limit = andromeda_observe::TRACE_QUERY_DEFAULT_LIMIT;
    let mut offset = 0usize;
    let mut include_total_count = false;
    let mut lsn_start = None;
    let mut lsn_end = None;
    let mut durable_family_filter = None;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            JSON_FLAG => {
                json_output = true;
                index += 1;
            },
            DIAGNOSTIC_JSON_FLAG => {
                json_output = true;
                diagnostic_json = true;
                index += 1;
            },
            "--journal" => {
                journal_path = Some(PathBuf::from(next_value(
                    args,
                    &mut index,
                    "inspect",
                    "--journal",
                )?));
            },
            "--trace-id" => {
                let value = parse_u128(
                    next_value(args, &mut index, "inspect", "--trace-id")?,
                    "inspect",
                    "trace-id",
                )?;
                filter.trace_id = Some(TraceId::new(value));
            },
            "--principal" => {
                let principal = next_value(args, &mut index, "inspect", "--principal")?;
                if contains_sensitive_cli_evidence(&principal) {
                    return Err(cli_error(
                        "audit inspect principal filter must not contain secret-bearing credential material",
                    ));
                }
                filter.principal = Some(principal);
            },
            "--family" => {
                let family_filter = parse_audit_family_filter(&next_value(
                    args, &mut index, "inspect", "--family",
                )?)?;
                filter.family = Some(family_filter.trace_family);
                durable_family_filter = family_filter.durable_family;
            },
            "--limit" => {
                limit = parse_usize(
                    next_value(args, &mut index, "inspect", "--limit")?,
                    "inspect",
                    "limit",
                )?;
            },
            "--offset" => {
                offset = parse_usize(
                    next_value(args, &mut index, "inspect", "--offset")?,
                    "inspect",
                    "offset",
                )?;
            },
            "--lsn-start" => {
                lsn_start = Some(parse_u64(
                    next_value(args, &mut index, "inspect", "--lsn-start")?,
                    "inspect",
                    "lsn-start",
                )?);
            },
            "--lsn-end" => {
                lsn_end = Some(parse_u64(
                    next_value(args, &mut index, "inspect", "--lsn-end")?,
                    "inspect",
                    "lsn-end",
                )?);
            },
            "--lsn-range" => {
                let value = next_value(args, &mut index, "inspect", "--lsn-range")?;
                let (start, end) = parse_lsn_range(&value)?;
                if lsn_start.is_some() || lsn_end.is_some() {
                    return Err(cli_error(
                        "audit inspect cannot combine --lsn-range with --lsn-start/--lsn-end",
                    ));
                }
                lsn_start = Some(start);
                lsn_end = Some(end);
            },
            "--include-total-count" => {
                include_total_count = true;
                index += 1;
            },
            "-h" | "--help" => {
                return Err(cli_error(
                    "usage: andromeda-cli audit inspect [--journal <path>] [--trace-id <u128>] [--principal <id>] [--family <family>] [--lsn-range <start..end>|--lsn-start <lsn> --lsn-end <lsn>] [--limit <n>] [--offset <n>] [--include-total-count] [--json|--diagnostic-json]",
                ));
            },
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown audit inspect option; supported options are --journal, --trace-id, --principal, --family, --lsn-range, --lsn-start, --lsn-end, --limit, --offset, --include-total-count, --json, and --diagnostic-json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected audit inspect argument; filters must be passed with named options",
                ));
            },
        }
    }

    match (lsn_start, lsn_end) {
        (Some(start_lsn), Some(end_lsn)) => {
            filter.lsn_range = Some(TraceQueryLsnRange::new(start_lsn, end_lsn));
        },
        (Some(_), None) | (None, Some(_)) => {
            return Err(cli_error(
                "audit inspect LSN filter requires both --lsn-start and --lsn-end",
            ));
        },
        (None, None) => {},
    }

    let spec = TraceQuerySpec {
        filter,
        limit,
        offset,
        include_total_count,
    };
    spec.validate()?;

    Ok(AuditInspectionOptions {
        spec,
        durable_family_filter,
        json_output,
        diagnostic_json,
        journal_path,
    })
}

pub(super) fn parse_audit_verify_options(args: &[String]) -> AndromedaResult<AuditVerifyOptions> {
    let mut journal_path = None;
    let mut json_output = false;
    let mut diagnostic_json = false;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--journal" => {
                journal_path = Some(PathBuf::from(next_value(
                    args,
                    &mut index,
                    "verify",
                    "--journal",
                )?));
            },
            JSON_FLAG => {
                json_output = true;
                index += 1;
            },
            DIAGNOSTIC_JSON_FLAG => {
                json_output = true;
                diagnostic_json = true;
                index += 1;
            },
            "-h" | "--help" => {
                return Err(cli_error(
                    "usage: andromeda-cli audit verify --journal <path> [--json|--diagnostic-json]",
                ));
            },
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown audit verify option; supported options are --journal, --json, and --diagnostic-json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected audit verify argument; verification inputs must be passed with named options",
                ));
            },
        }
    }

    Ok(AuditVerifyOptions {
        journal_path,
        json_output,
        diagnostic_json,
    })
}

pub(super) fn parse_audit_compact_options(args: &[String]) -> AndromedaResult<AuditCompactOptions> {
    let mut journal_path = None;
    let mut retain_from_lsn = None;
    let mut preserve_forensic_hold = true;
    let mut diagnostic_json = false;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--journal" => {
                journal_path = Some(PathBuf::from(next_value(
                    args,
                    &mut index,
                    "compact",
                    "--journal",
                )?));
            },
            "--retain-from-lsn" => {
                retain_from_lsn = Some(parse_u64(
                    next_value(args, &mut index, "compact", "--retain-from-lsn")?,
                    "compact",
                    "retain-from-lsn",
                )?);
            },
            "--preserve-forensic-hold" => {
                preserve_forensic_hold = true;
                index += 1;
            },
            DIAGNOSTIC_JSON_FLAG => {
                diagnostic_json = true;
                index += 1;
            },
            JSON_FLAG => {
                return Err(cli_error(
                    "audit compact uses --diagnostic-json to make JSON diagnostic-only explicit",
                ));
            },
            "-h" | "--help" => {
                return Err(cli_error(
                    "usage: andromeda-cli audit compact --retain-from-lsn <lsn> [--journal <path>] [--preserve-forensic-hold] [--diagnostic-json]",
                ));
            },
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown audit compact option; supported options are --journal, --retain-from-lsn, --preserve-forensic-hold, and --diagnostic-json",
                ));
            },
            _ => {
                return Err(cli_error(
                    "unexpected audit compact argument; compaction policy must be passed with named options",
                ));
            },
        }
    }

    let Some(retain_from_lsn) = retain_from_lsn else {
        return Err(cli_error(
            "audit compact requires --retain-from-lsn <non-zero-lsn>",
        ));
    };

    Ok(AuditCompactOptions {
        journal_path,
        retain_from_lsn,
        preserve_forensic_hold,
        diagnostic_json,
    })
}

fn next_value(
    args: &[String],
    index: &mut usize,
    command: &str,
    option: &str,
) -> AndromedaResult<String> {
    let value_index = index.saturating_add(1);
    let Some(value) = args.get(value_index) else {
        return Err(cli_error(format!(
            "audit {command} option {option} requires a value"
        )));
    };
    if value.starts_with("--") {
        return Err(cli_error(format!(
            "audit {command} option {option} requires a value"
        )));
    }
    *index += 2;
    Ok(value.clone())
}

fn parse_u128(value: String, command: &str, label: &str) -> AndromedaResult<u128> {
    value.parse::<u128>().map_err(|_| {
        cli_error(format!(
            "audit {command} {label} must be an unsigned integer"
        ))
    })
}

fn parse_u64(value: String, command: &str, label: &str) -> AndromedaResult<u64> {
    value.parse::<u64>().map_err(|_| {
        cli_error(format!(
            "audit {command} {label} must be an unsigned integer"
        ))
    })
}

fn parse_usize(value: String, command: &str, label: &str) -> AndromedaResult<usize> {
    let parsed = value.parse::<usize>().map_err(|_| {
        cli_error(format!(
            "audit {command} {label} must be an unsigned integer"
        ))
    })?;
    if label == "limit" && parsed > TRACE_QUERY_MAX_LIMIT {
        return Err(cli_error(
            "audit inspect limit exceeds TRACE_QUERY_MAX_LIMIT",
        ));
    }
    Ok(parsed)
}

fn parse_lsn_range(value: &str) -> AndromedaResult<(u64, u64)> {
    let (start, end) = value
        .split_once("..=")
        .or_else(|| value.split_once(".."))
        .ok_or_else(|| cli_error("audit inspect --lsn-range must use start..end syntax"))?;
    let start = parse_u64(start.to_string(), "inspect", "lsn-range start")?;
    let end = parse_u64(end.to_string(), "inspect", "lsn-range end")?;
    if start == 0 || end == 0 {
        return Err(cli_error(
            "audit inspect --lsn-range endpoints must be non-zero",
        ));
    }
    if start > end {
        return Err(cli_error(
            "audit inspect --lsn-range must have start_lsn <= end_lsn",
        ));
    }
    Ok((start, end))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedAuditFamilyFilter {
    trace_family: TraceEventFamily,
    durable_family: Option<DurableAuditEventFamily>,
}

fn parse_audit_family_filter(value: &str) -> AndromedaResult<ParsedAuditFamilyFilter> {
    match value {
        "decision" => Ok(trace_family_filter(TraceEventFamily::Decision)),
        "procedure-invocation" => Ok(trace_family_filter(TraceEventFamily::ProcedureInvocation)),
        "wal" => Ok(trace_family_filter(TraceEventFamily::Wal)),
        "recovery" => Ok(trace_family_filter(TraceEventFamily::Recovery)),
        "manifest-catalog" => Ok(trace_family_filter(TraceEventFamily::ManifestCatalog)),
        "protocol" => Ok(trace_family_filter(TraceEventFamily::Protocol)),
        "security-audit" => Ok(trace_family_filter(TraceEventFamily::SecurityAudit)),
        "admin-audit" => Ok(trace_family_filter(TraceEventFamily::AdminAudit)),
        "resource" => Ok(trace_family_filter(TraceEventFamily::Resource)),
        "io" => Ok(trace_family_filter(TraceEventFamily::Io)),
        "gpu" => Ok(trace_family_filter(TraceEventFamily::Gpu)),
        "transaction" => Ok(trace_family_filter(TraceEventFamily::Transaction)),
        "admin" | "admin-decision" => Ok(exact_admin_audit_family_filter(
            DurableAuditEventFamily::AdminDecision,
        )),
        "hadr" | "hadr-decision" => Ok(exact_admin_audit_family_filter(
            DurableAuditEventFamily::HadrDecision,
        )),
        "backup" | "backup-decision" => Ok(exact_admin_audit_family_filter(
            DurableAuditEventFamily::BackupDecision,
        )),
        "restore" | "restore-decision" => Ok(exact_admin_audit_family_filter(
            DurableAuditEventFamily::RestoreDecision,
        )),
        _ => Err(cli_error(
            "unknown audit inspect family; expected decision, procedure-invocation, wal, recovery, manifest-catalog, protocol, security-audit, admin-audit, admin, hadr, backup, restore, resource, io, gpu, or transaction",
        )),
    }
}

const fn trace_family_filter(trace_family: TraceEventFamily) -> ParsedAuditFamilyFilter {
    ParsedAuditFamilyFilter {
        trace_family,
        durable_family: None,
    }
}

const fn exact_admin_audit_family_filter(
    durable_family: DurableAuditEventFamily,
) -> ParsedAuditFamilyFilter {
    ParsedAuditFamilyFilter {
        trace_family: TraceEventFamily::AdminAudit,
        durable_family: Some(durable_family),
    }
}
