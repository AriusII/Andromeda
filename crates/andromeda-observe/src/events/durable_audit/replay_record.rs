use andromeda_core::AndromedaResult;

use crate::events::{contains_sensitive_marker, observe_error};

use super::{
    DurableAuditEventFamily, DurableAuditPrincipalBinding, DurableAuditReplayQuery,
    DurableAuditSinkReport, validate_permissioned_critical_policy_binding,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditReplayRecord {
    pub report: DurableAuditSinkReport,
    pub principal_binding: DurableAuditPrincipalBinding,
    pub event_kind: String,
}

impl DurableAuditReplayRecord {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.report.validate()?;
        self.principal_binding.validate()?;
        if self.event_kind.trim().is_empty() {
            return Err(observe_error(
                "durable audit replay record requires event kind evidence",
            ));
        }
        if contains_sensitive_marker(&self.event_kind) {
            return Err(observe_error(
                "durable audit replay event kind must not contain secret evidence",
            ));
        }
        if self.report.identity.family == DurableAuditEventFamily::SecurityDecision
            && self.principal_binding.policy_version.is_none()
        {
            return Err(observe_error(
                "durable audit security replay record requires policy version evidence",
            ));
        }
        validate_permissioned_critical_policy_binding(
            self.report.identity.family,
            &self.principal_binding,
            "durable audit replay records",
        )?;
        Ok(())
    }

    pub fn matches_replay_filter(&self, query: &DurableAuditReplayQuery) -> bool {
        if let Some(family) = query.family
            && self.report.identity.family != family
        {
            return false;
        }
        if let Some(trace_id) = query.trace_id
            && self.report.identity.trace_id != trace_id
        {
            return false;
        }
        if let Some(principal_id) = &query.principal_id
            && self.principal_binding.principal_id != *principal_id
        {
            return false;
        }
        if let Some(range) = query.lsn_range
            && !range.contains(self.report.evidence.record_lsn)
            && !range.contains(self.report.evidence.durable_lsn)
        {
            return false;
        }
        true
    }

    pub fn matches_query(&self, query: &DurableAuditReplayQuery) -> bool {
        self.matches_replay_filter(query)
    }
}
