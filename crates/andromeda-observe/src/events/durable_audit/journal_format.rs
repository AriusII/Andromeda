use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path,
};

use andromeda_core::{AndromedaResult, RequestId, SessionId};

use super::{
    DurableAuditEventFamily, DurableAuditFailureKind, DurableAuditPrincipalBinding,
    DurableAuditRecordIdentity, DurableAuditReplayBehavior, DurableAuditReplayEvidence,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayResult,
    DurableAuditReplayWindow, DurableAuditRetentionBoundary, DurableAuditSinkReport,
    DurableAuditSinkResult, DurableAuditWalEvidence, checksum64, sink_failure,
};
use crate::{
    TraceId,
    events::{EventId, Permission, SurfaceScope, observe_error},
};

const JOURNAL_PREFIX: &str = "andromeda-durable-audit-v1";
const NONE_FIELD: &str = "-";

pub(super) fn replay_durable_audit_journal(
    path: &Path,
    query: &DurableAuditReplayQuery,
) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
    Ok(
        replay_durable_audit_journal_with_evidence(path, query, DurableAuditReplayWindow::ALL)?
            .records,
    )
}

pub(super) fn replay_durable_audit_journal_with_evidence(
    path: &Path,
    query: &DurableAuditReplayQuery,
    window: DurableAuditReplayWindow,
) -> DurableAuditSinkResult<DurableAuditReplayResult> {
    query.validate().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::ValidationRejected,
            None,
            error.message().to_string(),
        )
    })?;
    window.validate().map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::ValidationRejected,
            None,
            error.message().to_string(),
        )
    })?;

    if !path.exists() {
        return Ok(DurableAuditReplayResult {
            evidence: DurableAuditReplayEvidence::empty(query, window),
            records: Vec::new(),
        });
    }

    let file = File::open(path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal: {error}"),
        )
    })?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut previous_lsn = 0u64;
    let mut records_scanned = 0usize;
    let mut records_matched = 0usize;
    let mut skipped = 0usize;
    let mut first_returned_lsn = None;
    let mut last_returned_lsn = None;

    for (line_index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!("failed to read durable audit journal line: {error}"),
            )
        })?;
        let record = parse_journal_line(&line).map_err(|reason| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!(
                    "durable audit journal corruption at line {}: {reason}",
                    line_index + 1
                ),
            )
        })?;
        records_scanned = records_scanned.saturating_add(1);
        if record.report.evidence.record_lsn <= previous_lsn {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                Some(record.report.identity),
                "durable audit journal record LSNs must increase strictly",
            ));
        }
        previous_lsn = record.report.evidence.record_lsn;

        if record.matches_query(query) {
            records_matched = records_matched.saturating_add(1);
            if skipped < window.offset {
                skipped = skipped.saturating_add(1);
                continue;
            }
            if records.len() < window.limit {
                let record_lsn = record.report.evidence.record_lsn;
                first_returned_lsn.get_or_insert(record_lsn);
                last_returned_lsn = Some(record_lsn);
                records.push(record);
            }
        }
    }

    let records_returned = records.len();
    Ok(DurableAuditReplayResult {
        evidence: DurableAuditReplayEvidence {
            records_scanned,
            records_matched,
            records_returned,
            filter_applied: query.has_filter(),
            limit: window.limit,
            offset: window.offset,
            truncated: records_matched.saturating_sub(window.offset) > records_returned,
            first_returned_lsn,
            last_returned_lsn,
        },
        records,
    })
}

pub(super) fn next_record_lsn(
    path: &Path,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<u64> {
    let len = fs::metadata(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::WalAppendRejected,
                identity,
                format!("failed to inspect durable audit journal length: {error}"),
            )
        })?
        .len();
    if len == 0 {
        return Ok(1);
    }

    replay_durable_audit_journal(path, &DurableAuditReplayQuery::all())?
        .last()
        .map(|record| record.report.evidence.record_lsn.saturating_add(1))
        .ok_or_else(|| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                identity,
                "durable audit journal contains bytes but no replayable records",
            )
        })
}

pub(super) fn journal_line(record: &DurableAuditReplayRecord) -> AndromedaResult<String> {
    let payload = journal_payload(record)?;
    let checksum = checksum64(payload.as_bytes());
    if checksum != record.report.evidence.checksum {
        return Err(observe_error(
            "durable audit journal checksum must match report evidence",
        ));
    }
    Ok(format!("{payload}|checksum={checksum:016x}\n"))
}

pub(super) fn journal_payload(record: &DurableAuditReplayRecord) -> AndromedaResult<String> {
    record.validate()?;
    let report = record.report;
    let binding = &record.principal_binding;
    Ok(format!(
        "{JOURNAL_PREFIX}|record_lsn={}|durable_lsn={}|event_id={}|trace_id={}|family={}|sequence={}|retention={}|replay={}|principal_id={}|certificate_fingerprint={}|surface={}|permission={}|request_id={}|session_id={}|event_kind={}",
        report.evidence.record_lsn,
        report.evidence.durable_lsn,
        report.identity.event_id.get(),
        report.identity.trace_id.get(),
        format_family(report.identity.family),
        report.identity.sequence_number,
        format_retention(report.retention),
        format_replay(report.replay_behavior),
        encode_string(&binding.principal_id),
        encode_optional_string(binding.certificate_fingerprint.as_deref()),
        binding.surface.map(format_surface).unwrap_or(NONE_FIELD),
        binding
            .permission
            .map(format_permission)
            .unwrap_or(NONE_FIELD),
        binding
            .request_id
            .map(|request_id| request_id.get().to_string())
            .unwrap_or_else(|| NONE_FIELD.to_string()),
        binding
            .session_id
            .map(|session_id| session_id.get().to_string())
            .unwrap_or_else(|| NONE_FIELD.to_string()),
        encode_string(&record.event_kind),
    ))
}

fn parse_journal_line(line: &str) -> Result<DurableAuditReplayRecord, String> {
    let (payload, checksum_field) = line
        .rsplit_once("|checksum=")
        .ok_or_else(|| "missing checksum field".to_string())?;
    if !payload.is_ascii() {
        return Err("durable audit journal payload must be ASCII".to_string());
    }
    if checksum_field.len() != 16 || !checksum_field.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("checksum field must be 16 ASCII hex characters".to_string());
    }
    let expected_checksum = u64::from_str_radix(checksum_field, 16)
        .map_err(|_| "checksum field is not valid hex".to_string())?;
    let actual_checksum = checksum64(payload.as_bytes());
    if actual_checksum != expected_checksum {
        return Err("checksum mismatch".to_string());
    }

    let fields: Vec<&str> = payload.split('|').collect();
    if fields.len() != 16 {
        return Err("unexpected durable audit journal field count".to_string());
    }
    if fields[0] != JOURNAL_PREFIX {
        return Err("unexpected durable audit journal prefix".to_string());
    }

    let record_lsn = parse_u64_field(fields[1], "record_lsn")?;
    let durable_lsn = parse_u64_field(fields[2], "durable_lsn")?;
    let event_id = parse_u128_field(fields[3], "event_id")?;
    let trace_id = parse_u128_field(fields[4], "trace_id")?;
    let family = parse_family(strip_field(fields[5], "family")?)?;
    let sequence_number = parse_u64_field(fields[6], "sequence")?;
    let retention = parse_retention(strip_field(fields[7], "retention")?)?;
    let replay_behavior = parse_replay(strip_field(fields[8], "replay")?)?;
    let principal_id = decode_string(strip_field(fields[9], "principal_id")?)?;
    let certificate_fingerprint =
        decode_optional_string(strip_field(fields[10], "certificate_fingerprint")?)?;
    let surface = parse_optional_surface(strip_field(fields[11], "surface")?)?;
    let permission = parse_optional_permission(strip_field(fields[12], "permission")?)?;
    let request_id =
        parse_optional_u64(strip_field(fields[13], "request_id")?)?.map(RequestId::new);
    let session_id =
        parse_optional_u64(strip_field(fields[14], "session_id")?)?.map(SessionId::new);
    let event_kind = decode_string(strip_field(fields[15], "event_kind")?)?;

    let record = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: DurableAuditRecordIdentity {
                event_id: EventId::new(event_id),
                trace_id: TraceId::new(trace_id),
                family,
                sequence_number,
            },
            evidence: DurableAuditWalEvidence {
                record_lsn,
                durable_lsn,
                checksum: expected_checksum,
            },
            replay_behavior,
            retention,
        },
        principal_binding: DurableAuditPrincipalBinding {
            principal_id,
            certificate_fingerprint,
            surface,
            permission,
            request_id,
            session_id,
        },
        event_kind,
    };
    record
        .validate()
        .map_err(|error| error.message().to_string())?;
    Ok(record)
}

fn parse_u64_field(field: &str, label: &str) -> Result<u64, String> {
    strip_field(field, label)?
        .parse::<u64>()
        .map_err(|_| format!("{label} field is not a valid u64"))
}

fn parse_u128_field(field: &str, label: &str) -> Result<u128, String> {
    strip_field(field, label)?
        .parse::<u128>()
        .map_err(|_| format!("{label} field is not a valid u128"))
}

fn parse_optional_u64(value: &str) -> Result<Option<u64>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| "optional integer field is not valid u64".to_string())
}

fn strip_field<'a>(field: &'a str, label: &str) -> Result<&'a str, String> {
    field
        .strip_prefix(label)
        .and_then(|rest| rest.strip_prefix('='))
        .ok_or_else(|| format!("missing {label} field"))
}

fn encode_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn encode_optional_string(value: Option<&str>) -> String {
    value
        .map(encode_string)
        .unwrap_or_else(|| NONE_FIELD.to_string())
}

fn decode_string(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = decode_hex_nibble(pair[0])?;
        let low = decode_hex_nibble(pair[1])?;
        bytes.push((high << 4) | low);
    }
    String::from_utf8(bytes).map_err(|_| "hex string is not valid UTF-8".to_string())
}

fn decode_hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("hex string contains non-hex bytes".to_string()),
    }
}

fn decode_optional_string(value: &str) -> Result<Option<String>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    decode_string(value).map(Some)
}

fn format_family(value: DurableAuditEventFamily) -> &'static str {
    match value {
        DurableAuditEventFamily::SecurityDecision => "SecurityDecision",
        DurableAuditEventFamily::AdminDecision => "AdminDecision",
        DurableAuditEventFamily::AdmissionDecision => "AdmissionDecision",
        DurableAuditEventFamily::CatalogDecision => "CatalogDecision",
        DurableAuditEventFamily::HadrDecision => "HadrDecision",
        DurableAuditEventFamily::BackupDecision => "BackupDecision",
        DurableAuditEventFamily::RestoreDecision => "RestoreDecision",
        DurableAuditEventFamily::ForensicDecision => "ForensicDecision",
        DurableAuditEventFamily::RecoveryDecision => "RecoveryDecision",
        DurableAuditEventFamily::GenericAudit => "GenericAudit",
    }
}

fn parse_family(value: &str) -> Result<DurableAuditEventFamily, String> {
    match value {
        "SecurityDecision" => Ok(DurableAuditEventFamily::SecurityDecision),
        "AdminDecision" => Ok(DurableAuditEventFamily::AdminDecision),
        "AdmissionDecision" => Ok(DurableAuditEventFamily::AdmissionDecision),
        "CatalogDecision" => Ok(DurableAuditEventFamily::CatalogDecision),
        "HadrDecision" => Ok(DurableAuditEventFamily::HadrDecision),
        "BackupDecision" => Ok(DurableAuditEventFamily::BackupDecision),
        "RestoreDecision" => Ok(DurableAuditEventFamily::RestoreDecision),
        "ForensicDecision" => Ok(DurableAuditEventFamily::ForensicDecision),
        "RecoveryDecision" => Ok(DurableAuditEventFamily::RecoveryDecision),
        "GenericAudit" => Ok(DurableAuditEventFamily::GenericAudit),
        _ => Err("unknown durable audit family".to_string()),
    }
}

fn format_replay(value: DurableAuditReplayBehavior) -> &'static str {
    match value {
        DurableAuditReplayBehavior::ForensicOnly => "ForensicOnly",
        DurableAuditReplayBehavior::RebuildDecisionIndex => "RebuildDecisionIndex",
        DurableAuditReplayBehavior::CorruptionBoundary => "CorruptionBoundary",
    }
}

fn parse_replay(value: &str) -> Result<DurableAuditReplayBehavior, String> {
    match value {
        "ForensicOnly" => Ok(DurableAuditReplayBehavior::ForensicOnly),
        "RebuildDecisionIndex" => Ok(DurableAuditReplayBehavior::RebuildDecisionIndex),
        "CorruptionBoundary" => Ok(DurableAuditReplayBehavior::CorruptionBoundary),
        _ => Err("unknown durable audit replay behavior".to_string()),
    }
}

fn format_retention(value: DurableAuditRetentionBoundary) -> &'static str {
    match value {
        DurableAuditRetentionBoundary::WalSegment => "WalSegment",
        DurableAuditRetentionBoundary::CatalogVersion => "CatalogVersion",
        DurableAuditRetentionBoundary::SecurityPolicy => "SecurityPolicy",
        DurableAuditRetentionBoundary::ForensicHold => "ForensicHold",
    }
}

fn parse_retention(value: &str) -> Result<DurableAuditRetentionBoundary, String> {
    match value {
        "WalSegment" => Ok(DurableAuditRetentionBoundary::WalSegment),
        "CatalogVersion" => Ok(DurableAuditRetentionBoundary::CatalogVersion),
        "SecurityPolicy" => Ok(DurableAuditRetentionBoundary::SecurityPolicy),
        "ForensicHold" => Ok(DurableAuditRetentionBoundary::ForensicHold),
        _ => Err("unknown durable audit retention boundary".to_string()),
    }
}

fn format_surface(value: SurfaceScope) -> &'static str {
    match value {
        SurfaceScope::Application => "Application",
        SurfaceScope::Administration => "Administration",
        SurfaceScope::Cluster => "Cluster",
        SurfaceScope::BackupAgent => "BackupAgent",
        SurfaceScope::MonitoringAgent => "MonitoringAgent",
    }
}

fn parse_optional_surface(value: &str) -> Result<Option<SurfaceScope>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    match value {
        "Application" => Ok(Some(SurfaceScope::Application)),
        "Administration" => Ok(Some(SurfaceScope::Administration)),
        "Cluster" => Ok(Some(SurfaceScope::Cluster)),
        "BackupAgent" => Ok(Some(SurfaceScope::BackupAgent)),
        "MonitoringAgent" => Ok(Some(SurfaceScope::MonitoringAgent)),
        _ => Err("unknown durable audit surface scope".to_string()),
    }
}

fn format_permission(value: Permission) -> &'static str {
    match value {
        Permission::ExecuteProcedure => "ExecuteProcedure",
        Permission::ReadContract => "ReadContract",
        Permission::CreateTable => "CreateTable",
        Permission::CreateMap => "CreateMap",
        Permission::CreateProcedure => "CreateProcedure",
        Permission::ImportDefinitionBatch => "ImportDefinitionBatch",
        Permission::DebugProcedure => "DebugProcedure",
        Permission::ReadProcedureStore => "ReadProcedureStore",
        Permission::InspectPlans => "InspectPlans",
        Permission::ManageSecurity => "ManageSecurity",
        Permission::RotateCertificate => "RotateCertificate",
        Permission::RevokeCertificateIdentity => "RevokeCertificateIdentity",
        Permission::Backup => "Backup",
        Permission::Restore => "Restore",
        Permission::ForensicStart => "ForensicStart",
        Permission::ClusterPromote => "ClusterPromote",
        Permission::FenceNode => "FenceNode",
        Permission::UpdateClusterManifest => "UpdateClusterManifest",
    }
}

fn parse_optional_permission(value: &str) -> Result<Option<Permission>, String> {
    if value == NONE_FIELD {
        return Ok(None);
    }
    match value {
        "ExecuteProcedure" => Ok(Some(Permission::ExecuteProcedure)),
        "ReadContract" => Ok(Some(Permission::ReadContract)),
        "CreateTable" => Ok(Some(Permission::CreateTable)),
        "CreateMap" => Ok(Some(Permission::CreateMap)),
        "CreateProcedure" => Ok(Some(Permission::CreateProcedure)),
        "ImportDefinitionBatch" => Ok(Some(Permission::ImportDefinitionBatch)),
        "DebugProcedure" => Ok(Some(Permission::DebugProcedure)),
        "ReadProcedureStore" => Ok(Some(Permission::ReadProcedureStore)),
        "InspectPlans" => Ok(Some(Permission::InspectPlans)),
        "ManageSecurity" => Ok(Some(Permission::ManageSecurity)),
        "RotateCertificate" => Ok(Some(Permission::RotateCertificate)),
        "RevokeCertificateIdentity" => Ok(Some(Permission::RevokeCertificateIdentity)),
        "Backup" => Ok(Some(Permission::Backup)),
        "Restore" => Ok(Some(Permission::Restore)),
        "ForensicStart" => Ok(Some(Permission::ForensicStart)),
        "ClusterPromote" => Ok(Some(Permission::ClusterPromote)),
        "FenceNode" => Ok(Some(Permission::FenceNode)),
        "UpdateClusterManifest" => Ok(Some(Permission::UpdateClusterManifest)),
        _ => Err("unknown durable audit permission".to_string()),
    }
}
