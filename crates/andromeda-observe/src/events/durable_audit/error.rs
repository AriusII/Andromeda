use andromeda_error::AndromedaResult;

use crate::events::{contains_sensitive_marker, observe_error};

use super::{DurableAuditFailureKind, DurableAuditRecordIdentity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditSinkFailure {
    pub kind: DurableAuditFailureKind,
    pub identity: Option<DurableAuditRecordIdentity>,
    pub reason: String,
}

impl DurableAuditSinkFailure {
    pub fn new(
        kind: DurableAuditFailureKind,
        identity: Option<DurableAuditRecordIdentity>,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let failure = Self {
            kind,
            identity,
            reason: reason.into(),
        };
        failure.validate()?;
        Ok(failure)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if let Some(identity) = self.identity {
            identity.validate()?;
        }
        if self.reason.trim().is_empty() {
            return Err(observe_error(
                "durable audit sink failure requires non-empty reason evidence",
            ));
        }
        if contains_sensitive_marker(&self.reason) {
            return Err(observe_error(
                "durable audit sink failure reason must not contain secret evidence",
            ));
        }
        Ok(())
    }

    pub fn requires_fail_closed(&self) -> bool {
        self.kind.requires_fail_closed()
    }
}

pub type DurableAuditSinkResult<T> = Result<T, DurableAuditSinkFailure>;

pub(crate) fn sink_failure(
    kind: DurableAuditFailureKind,
    identity: Option<DurableAuditRecordIdentity>,
    reason: impl Into<String>,
) -> DurableAuditSinkFailure {
    let identity = identity.filter(|identity| identity.validate().is_ok());
    let reason = reason.into();
    let reason = if reason.trim().is_empty() || contains_sensitive_marker(&reason) {
        "durable audit sink operation failed".to_string()
    } else {
        reason
    };

    DurableAuditSinkFailure::new(kind, identity, reason).unwrap_or(DurableAuditSinkFailure {
        kind,
        identity: None,
        reason: "durable audit sink operation failed".to_string(),
    })
}
