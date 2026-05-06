use andromeda_core::AndromedaResult;

use crate::TraceId;
use crate::events::{contains_sensitive_marker, observe_error};

use super::DurableAuditEventFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayLsnRange {
    pub start_lsn: u64,
    pub end_lsn: u64,
}

impl DurableAuditReplayLsnRange {
    pub const fn new(start_lsn: u64, end_lsn: u64) -> Self {
        Self { start_lsn, end_lsn }
    }

    pub const fn contains(self, lsn: u64) -> bool {
        self.start_lsn <= lsn && lsn <= self.end_lsn
    }

    pub const fn is_valid(self) -> bool {
        self.start_lsn != 0 && self.end_lsn != 0 && self.start_lsn <= self.end_lsn
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DurableAuditReplayQuery {
    pub family: Option<DurableAuditEventFamily>,
    pub trace_id: Option<TraceId>,
    pub principal_id: Option<String>,
    pub lsn_range: Option<DurableAuditReplayLsnRange>,
}

impl DurableAuditReplayQuery {
    pub fn all() -> Self {
        Self::default()
    }

    pub const fn has_filter(&self) -> bool {
        self.family.is_some()
            || self.trace_id.is_some()
            || self.principal_id.is_some()
            || self.lsn_range.is_some()
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if let Some(trace_id) = self.trace_id
            && trace_id.is_zero()
        {
            return Err(observe_error(
                "durable audit replay trace_id filter must be non-zero when present",
            ));
        }
        if let Some(principal_id) = &self.principal_id {
            if principal_id.trim().is_empty() {
                return Err(observe_error(
                    "durable audit replay principal filter must be non-empty when present",
                ));
            }
            if contains_sensitive_marker(principal_id) {
                return Err(observe_error(
                    "durable audit replay principal filter must not contain secret evidence",
                ));
            }
        }
        if let Some(range) = self.lsn_range
            && !range.is_valid()
        {
            return Err(observe_error(
                "durable audit replay LSN range must be non-zero and start_lsn <= end_lsn",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayWindow {
    pub limit: usize,
    pub offset: usize,
}

impl DurableAuditReplayWindow {
    pub const ALL: Self = Self {
        limit: usize::MAX,
        offset: 0,
    };

    pub const fn new(limit: usize, offset: usize) -> Self {
        Self { limit, offset }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.limit == 0 {
            return Err(observe_error(
                "durable audit replay window limit must be non-zero",
            ));
        }
        Ok(())
    }
}

impl Default for DurableAuditReplayWindow {
    fn default() -> Self {
        Self::ALL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditReplayEvidence {
    pub records_scanned: usize,
    pub records_matched: usize,
    pub records_returned: usize,
    pub filter_applied: bool,
    pub limit: usize,
    pub offset: usize,
    pub truncated: bool,
    pub first_returned_lsn: Option<u64>,
    pub last_returned_lsn: Option<u64>,
}

impl DurableAuditReplayEvidence {
    pub const fn empty(query: &DurableAuditReplayQuery, window: DurableAuditReplayWindow) -> Self {
        Self {
            records_scanned: 0,
            records_matched: 0,
            records_returned: 0,
            filter_applied: query.has_filter(),
            limit: window.limit,
            offset: window.offset,
            truncated: false,
            first_returned_lsn: None,
            last_returned_lsn: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditReplayResult {
    pub evidence: DurableAuditReplayEvidence,
    pub records: Vec<super::DurableAuditReplayRecord>,
}
