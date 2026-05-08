use andromeda_error::AndromedaResult;
use andromeda_types::{RequestId, SessionId};

use super::super::{
    DurableAuditPrincipalBinding, DurableAuditRecordIdentity, DurableAuditReplayRecord,
    DurableAuditSinkReport, DurableAuditWalEvidence, checksum64,
};
use super::{
    DurableAuditJournalLine, JOURNAL_PREFIX, JOURNAL_PREFIX_V1,
    fields::{
        decode_optional_string, decode_string, encode_optional_string, encode_string,
        format_family, format_permission, format_replay, format_retention, format_surface,
        parse_family, parse_hex_checksum, parse_optional_permission, parse_optional_policy_version,
        parse_optional_surface, parse_optional_u64, parse_replay, parse_retention, parse_u64_field,
        parse_u128_field, strip_field,
    },
};
use crate::{
    TraceId,
    events::{EventId, observe_error},
};

pub(super) struct DurableAuditJournalRecord {
    pub(super) record: DurableAuditReplayRecord,
    pub(super) chain_checksum: u64,
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

pub(in crate::events::durable_audit) fn journal_line(
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

pub(in crate::events::durable_audit) fn journal_payload(
    record: &DurableAuditReplayRecord,
) -> AndromedaResult<String> {
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
        binding
            .surface
            .map(format_surface)
            .unwrap_or(super::NONE_FIELD),
        binding
            .permission
            .map(format_permission)
            .unwrap_or(super::NONE_FIELD),
        binding
            .policy_version
            .as_ref()
            .map(|policy| policy.policy_version.to_string())
            .unwrap_or_else(|| super::NONE_FIELD.to_string()),
        encode_optional_string(
            binding
                .policy_version
                .as_ref()
                .map(|policy| policy.policy_digest.as_str()),
        ),
        binding
            .request_id
            .map(|request_id| request_id.get().to_string())
            .unwrap_or_else(|| super::NONE_FIELD.to_string()),
        binding
            .session_id
            .map(|session_id| session_id.get().to_string())
            .unwrap_or_else(|| super::NONE_FIELD.to_string()),
        encode_string(&record.event_kind),
    ))
}

pub(super) fn parse_journal_line(
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
