mod anchor;
mod fields;
mod line;
mod scan;

pub(super) use anchor::{remove_journal_chain_anchor, write_journal_chain_anchor};
pub(super) use line::{journal_line, journal_payload};
pub(super) use scan::{
    next_record_anchor, replay_durable_audit_journal, replay_durable_audit_journal_with_evidence,
};

use super::DurableAuditReplayResult;

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

struct DurableAuditJournalScan {
    result: DurableAuditReplayResult,
    last_chain_checksum: u64,
    first_record_lsn: Option<u64>,
    last_record_lsn: Option<u64>,
}
