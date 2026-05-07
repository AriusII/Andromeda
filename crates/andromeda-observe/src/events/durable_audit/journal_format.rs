use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
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
    events::{EventId, Permission, SecurityPolicyVersionEvidence, SurfaceScope, observe_error},
};

const JOURNAL_PREFIX_V1: &str = "andromeda-durable-audit-v1";
const JOURNAL_PREFIX: &str = "andromeda-durable-audit-v2";
const JOURNAL_ANCHOR_PREFIX: &str = "andromeda-durable-audit-chain-v1";
const NONE_FIELD: &str = "-";
const GENESIS_CHAIN_CHECKSUM: u64 = 0;

pub(super) struct DurableAuditJournalAppendAnchor {
    pub record_lsn: u64,
    pub previous_chain_checksum: u64,
    pub first_record_lsn: u64,
    pub prior_record_count: usize,
}

pub(super) struct DurableAuditJournalLine {
    pub text: String,
    pub chain_checksum: u64,
}

struct DurableAuditJournalRecord {
    record: DurableAuditReplayRecord,
    chain_checksum: u64,
}

struct DurableAuditJournalScan {
    result: DurableAuditReplayResult,
    last_chain_checksum: u64,
    first_record_lsn: Option<u64>,
    last_record_lsn: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DurableAuditJournalAnchor {
    first_record_lsn: u64,
    last_record_lsn: u64,
    record_count: usize,
    tail_chain_checksum: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DurableAuditJournalRecordFormat {
    V1,
    V2,
}

impl DurableAuditJournalRecordFormat {
    const fn field_count(self) -> usize {
        match self {
            Self::V1 => 16,
            Self::V2 => 18,
        }
    }

    const fn has_policy_version_evidence(self) -> bool {
        matches!(self, Self::V2)
    }
}

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
    Ok(scan_durable_audit_journal(path, query, window)?.result)
}

fn scan_durable_audit_journal(
    path: &Path,
    query: &DurableAuditReplayQuery,
    window: DurableAuditReplayWindow,
) -> DurableAuditSinkResult<DurableAuditJournalScan> {
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
        let anchor_path = journal_anchor_path(path);
        if anchor_path.exists() {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                "durable audit chain anchor exists without durable audit journal",
            ));
        }
        return Ok(DurableAuditJournalScan {
            result: DurableAuditReplayResult {
                evidence: DurableAuditReplayEvidence::empty(query, window),
                records: Vec::new(),
            },
            last_chain_checksum: GENESIS_CHAIN_CHECKSUM,
            first_record_lsn: None,
            last_record_lsn: None,
        });
    }

    validate_journal_tail_is_record_delimited(path)?;
    let chain_anchor_present = journal_anchor_path(path).exists();

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
    let mut first_scanned_lsn = None;
    let mut last_scanned_lsn = None;
    let mut previous_chain_checksum = GENESIS_CHAIN_CHECKSUM;

    for (line_index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!("failed to read durable audit journal line: {error}"),
            )
        })?;
        let journal_record =
            parse_journal_line(&line, previous_chain_checksum).map_err(|reason| {
                sink_failure(
                    DurableAuditFailureKind::CorruptionDetected,
                    None,
                    format!(
                        "durable audit journal corruption at line {}: {reason}",
                        line_index + 1
                    ),
                )
            })?;
        let record = journal_record.record;
        records_scanned = records_scanned.saturating_add(1);
        let record_lsn = record.report.evidence.record_lsn;
        first_scanned_lsn.get_or_insert(record_lsn);
        if record_lsn <= previous_lsn {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                Some(record.report.identity),
                "durable audit journal record LSNs must increase strictly",
            ));
        }
        previous_lsn = record_lsn;
        last_scanned_lsn = Some(record_lsn);
        previous_chain_checksum = journal_record.chain_checksum;

        if record.matches_replay_filter(query) {
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
    let scan = DurableAuditJournalScan {
        result: DurableAuditReplayResult {
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
                chain_anchor_present,
                first_scanned_lsn,
                last_scanned_lsn,
                tail_chain_checksum: previous_chain_checksum,
            },
            records,
        },
        last_chain_checksum: previous_chain_checksum,
        first_record_lsn: first_scanned_lsn,
        last_record_lsn: last_scanned_lsn,
    };
    validate_journal_anchor(path, &scan)?;
    Ok(scan)
}

pub(super) fn next_record_anchor(
    path: &Path,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<DurableAuditJournalAppendAnchor> {
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
        let anchor_path = journal_anchor_path(path);
        if anchor_path.exists() {
            return Err(sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                identity,
                "durable audit chain anchor exists for empty durable audit journal",
            ));
        }
        return Ok(DurableAuditJournalAppendAnchor {
            record_lsn: 1,
            previous_chain_checksum: GENESIS_CHAIN_CHECKSUM,
            first_record_lsn: 1,
            prior_record_count: 0,
        });
    }

    let scan = scan_durable_audit_journal(
        path,
        &DurableAuditReplayQuery::all(),
        DurableAuditReplayWindow::ALL,
    )?;
    scan.result
        .records
        .last()
        .map(|record| DurableAuditJournalAppendAnchor {
            record_lsn: record.report.evidence.record_lsn.saturating_add(1),
            previous_chain_checksum: scan.last_chain_checksum,
            first_record_lsn: scan
                .first_record_lsn
                .unwrap_or(record.report.evidence.record_lsn),
            prior_record_count: scan.result.evidence.records_scanned,
        })
        .ok_or_else(|| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                identity,
                "durable audit journal contains bytes but no replayable records",
            )
        })
}

pub(super) fn write_journal_chain_anchor(
    path: &Path,
    identity: Option<DurableAuditRecordIdentity>,
    failure_kind: DurableAuditFailureKind,
    first_record_lsn: u64,
    last_record_lsn: u64,
    record_count: usize,
    tail_chain_checksum: u64,
) -> DurableAuditSinkResult<()> {
    let anchor = DurableAuditJournalAnchor {
        first_record_lsn,
        last_record_lsn,
        record_count,
        tail_chain_checksum,
    };
    validate_anchor_shape(anchor).map_err(|reason| sink_failure(failure_kind, identity, reason))?;
    let line = anchor_line(anchor);
    let anchor_path = journal_anchor_path(path);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&anchor_path)
        .map_err(|error| {
            sink_failure(
                failure_kind,
                identity,
                format!("failed to open durable audit chain anchor: {error}"),
            )
        })?;
    file.write_all(line.as_bytes()).map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to write durable audit chain anchor: {error}"),
        )
    })?;
    file.sync_all().map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to flush durable audit chain anchor: {error}"),
        )
    })?;
    sync_parent_directory(&anchor_path, failure_kind, identity)?;
    Ok(())
}

pub(super) fn remove_journal_chain_anchor(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
) -> DurableAuditSinkResult<()> {
    let anchor_path = journal_anchor_path(path);
    match fs::remove_file(&anchor_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(sink_failure(
            failure_kind,
            None,
            format!("failed to remove durable audit chain anchor: {error}"),
        )),
    }
}

pub(super) fn journal_line(
    record: &DurableAuditReplayRecord,
    previous_chain_checksum: u64,
) -> AndromedaResult<DurableAuditJournalLine> {
    let payload = journal_payload(record)?;
    let checksum = checksum64(payload.as_bytes());
    if checksum != record.report.evidence.checksum {
        return Err(observe_error(
            "durable audit journal checksum must match report evidence",
        ));
    }
    let chain_checksum = journal_chain_checksum(&payload, checksum, previous_chain_checksum);
    Ok(DurableAuditJournalLine {
        text: format!(
            "{payload}|previous_chain_checksum={previous_chain_checksum:016x}|chain_checksum={chain_checksum:016x}|checksum={checksum:016x}\n"
        ),
        chain_checksum,
    })
}

pub(super) fn journal_payload(record: &DurableAuditReplayRecord) -> AndromedaResult<String> {
    record.validate()?;
    let report = record.report;
    let binding = &record.principal_binding;
    Ok(format!(
        "{JOURNAL_PREFIX}|record_lsn={}|durable_lsn={}|event_id={}|trace_id={}|family={}|sequence={}|retention={}|replay={}|principal_id={}|certificate_fingerprint={}|surface={}|permission={}|policy_version={}|policy_digest={}|request_id={}|session_id={}|event_kind={}",
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
            .policy_version
            .as_ref()
            .map(|policy| policy.policy_version.to_string())
            .unwrap_or_else(|| NONE_FIELD.to_string()),
        encode_optional_string(
            binding
                .policy_version
                .as_ref()
                .map(|policy| policy.policy_digest.as_str()),
        ),
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

fn parse_journal_line(
    line: &str,
    expected_previous_chain_checksum: u64,
) -> Result<DurableAuditJournalRecord, String> {
    let (chain_payload, checksum_field) = line
        .rsplit_once("|checksum=")
        .ok_or_else(|| "missing checksum field".to_string())?;
    let (payload_with_previous_checksum, chain_checksum_field) = chain_payload
        .rsplit_once("|chain_checksum=")
        .ok_or_else(|| "missing chain_checksum field".to_string())?;
    let (payload, previous_chain_checksum_field) = payload_with_previous_checksum
        .rsplit_once("|previous_chain_checksum=")
        .ok_or_else(|| "missing previous_chain_checksum field".to_string())?;

    if !payload.is_ascii() {
        return Err("durable audit journal payload must be ASCII".to_string());
    }
    let previous_chain_checksum =
        parse_hex_checksum(previous_chain_checksum_field, "previous_chain_checksum")?;
    if previous_chain_checksum != expected_previous_chain_checksum {
        return Err("checksum chain previous value mismatch".to_string());
    }
    let expected_chain_checksum = parse_hex_checksum(chain_checksum_field, "chain_checksum")?;
    let expected_checksum = parse_hex_checksum(checksum_field, "checksum")?;
    let actual_checksum = checksum64(payload.as_bytes());
    if actual_checksum != expected_checksum {
        return Err("durable audit journal record checksum mismatch".to_string());
    }
    let actual_chain_checksum =
        journal_chain_checksum(payload, expected_checksum, expected_previous_chain_checksum);
    if actual_chain_checksum != expected_chain_checksum {
        return Err("durable audit journal chain checksum mismatch".to_string());
    }

    let fields: Vec<&str> = payload.split('|').collect();
    let record_format = fields
        .first()
        .copied()
        .ok_or_else(|| "missing durable audit journal format version".to_string())
        .and_then(parse_journal_record_format)?;
    if fields.len() != record_format.field_count() {
        return Err(format!(
            "durable audit journal format version field count mismatch: expected {} fields",
            record_format.field_count()
        ));
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
    let (policy_version, request_id_index, session_id_index, event_kind_index) =
        if record_format.has_policy_version_evidence() {
            (
                parse_optional_policy_version(
                    strip_field(fields[13], "policy_version")?,
                    strip_field(fields[14], "policy_digest")?,
                )?,
                15,
                16,
                17,
            )
        } else {
            (None, 13, 14, 15)
        };
    let request_id = parse_optional_u64(strip_field(fields[request_id_index], "request_id")?)?
        .map(RequestId::new);
    let session_id = parse_optional_u64(strip_field(fields[session_id_index], "session_id")?)?
        .map(SessionId::new);
    let event_kind = decode_string(strip_field(fields[event_kind_index], "event_kind")?)?;

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
            policy_version,
            request_id,
            session_id,
        },
        event_kind,
    };
    record
        .validate()
        .map_err(|error| error.message().to_string())?;
    Ok(DurableAuditJournalRecord {
        record,
        chain_checksum: expected_chain_checksum,
    })
}

fn journal_chain_checksum(payload: &str, checksum: u64, previous_chain_checksum: u64) -> u64 {
    checksum64(format!("{previous_chain_checksum:016x}|{checksum:016x}|{payload}").as_bytes())
}

fn parse_journal_record_format(prefix: &str) -> Result<DurableAuditJournalRecordFormat, String> {
    match prefix {
        JOURNAL_PREFIX_V1 => Ok(DurableAuditJournalRecordFormat::V1),
        JOURNAL_PREFIX => Ok(DurableAuditJournalRecordFormat::V2),
        _ => Err(
            "unsupported durable audit journal format version; supported versions are v1 and v2"
                .to_string(),
        ),
    }
}

fn validate_journal_tail_is_record_delimited(path: &Path) -> DurableAuditSinkResult<()> {
    let len = fs::metadata(path)
        .map_err(|error| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!("failed to inspect durable audit journal length: {error}"),
            )
        })?
        .len();
    if len == 0 {
        return Ok(());
    }

    let mut file = File::open(path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal tail: {error}"),
        )
    })?;
    file.seek(SeekFrom::End(-1)).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to seek durable audit journal tail: {error}"),
        )
    })?;
    let mut last_byte = [0u8; 1];
    file.read_exact(&mut last_byte).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit journal tail: {error}"),
        )
    })?;
    if last_byte[0] != b'\n' {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit journal has a truncated tail: last record must be record-delimited",
        ));
    }
    Ok(())
}

fn validate_journal_anchor(
    path: &Path,
    scan: &DurableAuditJournalScan,
) -> DurableAuditSinkResult<()> {
    let anchor_path = journal_anchor_path(path);
    if !anchor_path.exists() {
        if scan.result.evidence.records_scanned == 0 {
            return Ok(());
        }
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor is required for non-empty durable audit journal",
        ));
    }

    if scan.result.evidence.records_scanned == 0 {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor exists for empty durable audit journal",
        ));
    }

    if scan.last_chain_checksum == GENESIS_CHAIN_CHECKSUM {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor cannot reference the genesis checksum for a non-empty journal",
        ));
    }

    if scan.first_record_lsn.is_none() || scan.last_record_lsn.is_none() {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor requires scanned LSN evidence",
        ));
    }

    if scan.result.evidence.tail_chain_checksum != scan.last_chain_checksum {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit replay evidence tail checksum mismatch",
        ));
    }

    if scan.result.evidence.first_scanned_lsn != scan.first_record_lsn
        || scan.result.evidence.last_scanned_lsn != scan.last_record_lsn
    {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit replay evidence scanned LSN mismatch",
        ));
    }

    if !scan.result.evidence.chain_anchor_present {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit replay evidence must report the persisted chain anchor",
        ));
    }

    let anchor_text = fs::read_to_string(&anchor_path).map_err(|error| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            format!("failed to read durable audit chain anchor: {error}"),
        )
    })?;
    if !anchor_text.ends_with('\n') {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor has a truncated tail: last record must be record-delimited",
        ));
    }
    let mut latest_anchor = None;
    for (line_index, line) in anchor_text.lines().enumerate() {
        latest_anchor = Some(parse_anchor_line(line).map_err(|reason| {
            sink_failure(
                DurableAuditFailureKind::CorruptionDetected,
                None,
                format!(
                    "durable audit chain anchor corruption at record {}: {reason}",
                    line_index + 1
                ),
            )
        })?);
    }
    let anchor = latest_anchor.ok_or_else(|| {
        sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor must contain at least one record",
        )
    })?;

    if scan.first_record_lsn != Some(anchor.first_record_lsn) {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor first record LSN mismatch",
        ));
    }
    if scan.last_record_lsn != Some(anchor.last_record_lsn) {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor last record LSN mismatch",
        ));
    }
    if scan.result.evidence.records_scanned != anchor.record_count {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor record count mismatch",
        ));
    }
    if scan.last_chain_checksum != anchor.tail_chain_checksum {
        return Err(sink_failure(
            DurableAuditFailureKind::CorruptionDetected,
            None,
            "durable audit chain anchor tail checksum mismatch",
        ));
    }
    Ok(())
}

fn journal_anchor_path(path: &Path) -> PathBuf {
    let mut anchor = path.as_os_str().to_os_string();
    anchor.push(".chain");
    PathBuf::from(anchor)
}

#[cfg(unix)]
fn sync_parent_directory(
    path: &Path,
    failure_kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = File::open(parent).map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to open durable audit parent directory for sync: {error}"),
        )
    })?;
    directory.sync_all().map_err(|error| {
        sink_failure(
            failure_kind,
            identity,
            format!("failed to sync durable audit parent directory: {error}"),
        )
    })
}

#[cfg(not(unix))]
fn sync_parent_directory(
    _path: &Path,
    _failure_kind: DurableAuditFailureKind,
    _identity: Option<DurableAuditRecordIdentity>,
) -> DurableAuditSinkResult<()> {
    Ok(())
}

fn anchor_line(anchor: DurableAuditJournalAnchor) -> String {
    let payload = anchor_payload(anchor);
    let checksum = checksum64(payload.as_bytes());
    format!("{payload}|checksum={checksum:016x}\n")
}

fn anchor_payload(anchor: DurableAuditJournalAnchor) -> String {
    format!(
        "{JOURNAL_ANCHOR_PREFIX}|first_record_lsn={}|last_record_lsn={}|record_count={}|tail_chain_checksum={:016x}",
        anchor.first_record_lsn,
        anchor.last_record_lsn,
        anchor.record_count,
        anchor.tail_chain_checksum,
    )
}

fn parse_anchor_line(line: &str) -> Result<DurableAuditJournalAnchor, String> {
    let (payload, checksum_field) = line
        .rsplit_once("|checksum=")
        .ok_or_else(|| "missing checksum field".to_string())?;
    if !payload.is_ascii() {
        return Err("durable audit chain anchor payload must be ASCII".to_string());
    }
    let expected_checksum = parse_hex_checksum(checksum_field, "checksum")?;
    let actual_checksum = checksum64(payload.as_bytes());
    if actual_checksum != expected_checksum {
        return Err("durable audit chain anchor checksum mismatch".to_string());
    }

    let fields: Vec<&str> = payload.split('|').collect();
    if fields.len() != 5 {
        return Err("unexpected durable audit chain anchor field count".to_string());
    }
    if fields[0] != JOURNAL_ANCHOR_PREFIX {
        return Err("unsupported durable audit chain anchor format version".to_string());
    }

    let anchor = DurableAuditJournalAnchor {
        first_record_lsn: parse_u64_field(fields[1], "first_record_lsn")?,
        last_record_lsn: parse_u64_field(fields[2], "last_record_lsn")?,
        record_count: parse_usize_field(fields[3], "record_count")?,
        tail_chain_checksum: parse_hex_checksum(
            strip_field(fields[4], "tail_chain_checksum")?,
            "tail_chain_checksum",
        )?,
    };
    validate_anchor_shape(anchor)?;
    Ok(anchor)
}

fn validate_anchor_shape(anchor: DurableAuditJournalAnchor) -> Result<(), String> {
    if anchor.first_record_lsn == 0 || anchor.last_record_lsn == 0 {
        return Err("durable audit chain anchor LSNs must be non-zero".to_string());
    }
    if anchor.first_record_lsn > anchor.last_record_lsn {
        return Err("durable audit chain anchor LSN range must be ordered".to_string());
    }
    if anchor.record_count == 0 {
        return Err("durable audit chain anchor record count must be non-zero".to_string());
    }
    if anchor.tail_chain_checksum == 0 {
        return Err("durable audit chain anchor tail checksum must be non-zero".to_string());
    }
    Ok(())
}

fn parse_hex_checksum(field: &str, label: &str) -> Result<u64, String> {
    if field.len() != 16 || !field.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} field must be 16 ASCII hex characters"));
    }
    u64::from_str_radix(field, 16).map_err(|_| format!("{label} field is not valid hex"))
}

fn parse_usize_field(field: &str, label: &str) -> Result<usize, String> {
    strip_field(field, label)?
        .parse::<usize>()
        .map_err(|_| format!("{label} field is not a valid usize"))
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

fn parse_optional_policy_version(
    policy_version_value: &str,
    policy_digest_value: &str,
) -> Result<Option<SecurityPolicyVersionEvidence>, String> {
    let policy_version = parse_optional_u64(policy_version_value)?;
    let policy_digest = decode_optional_string(policy_digest_value)?;
    match (policy_version, policy_digest) {
        (None, None) => Ok(None),
        (Some(policy_version), Some(policy_digest)) => {
            SecurityPolicyVersionEvidence::new(policy_version, policy_digest)
                .map(Some)
                .map_err(|error| error.message().to_string())
        }
        _ => Err(
            "durable audit policy version evidence requires both policy_version and policy_digest"
                .to_string(),
        ),
    }
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
    if value.len() % 2 != 0 {
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
