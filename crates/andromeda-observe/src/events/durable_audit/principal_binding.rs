use andromeda_error::AndromedaResult;
use andromeda_types::{RequestId, SessionId};

use crate::events::{
    Permission, SecurityPolicyVersionEvidence, SurfaceScope, contains_sensitive_marker,
    observe_error,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditPrincipalBinding {
    pub principal_id: String,
    pub certificate_fingerprint: Option<String>,
    pub surface: Option<SurfaceScope>,
    pub permission: Option<Permission>,
    pub policy_version: Option<SecurityPolicyVersionEvidence>,
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
}

impl DurableAuditPrincipalBinding {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.principal_id.trim().is_empty() {
            return Err(observe_error(
                "durable audit principal binding requires non-empty principal_id",
            ));
        }

        let certificate_is_empty = self
            .certificate_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| fingerprint.trim().is_empty());
        if certificate_is_empty {
            return Err(observe_error(
                "durable audit principal binding certificate fingerprint must not be empty when present",
            ));
        }

        if contains_sensitive_marker(&self.principal_id)
            || self
                .certificate_fingerprint
                .as_ref()
                .is_some_and(|fingerprint| contains_sensitive_marker(fingerprint))
            || self
                .policy_version
                .as_ref()
                .is_some_and(SecurityPolicyVersionEvidence::contains_sensitive_evidence)
        {
            return Err(observe_error(
                "durable audit principal binding must not contain secret evidence",
            ));
        }

        if self
            .policy_version
            .as_ref()
            .is_some_and(|evidence| !evidence.has_version_evidence())
        {
            return Err(observe_error(
                "durable audit principal binding policy version evidence must be non-zero and canonical",
            ));
        }

        if let (Some(surface), Some(permission)) = (self.surface, self.permission)
            && !surface.permits_permission(permission)
        {
            return Err(observe_error(
                "durable audit principal binding surface must permit permission evidence",
            ));
        }

        if self.request_id.is_some() != self.session_id.is_some() {
            return Err(observe_error(
                "durable audit principal binding request/session ids must be present together",
            ));
        }

        if self
            .request_id
            .is_some_and(|request_id| request_id.get() == 0)
            || self
                .session_id
                .is_some_and(|session_id| session_id.get() == 0)
        {
            return Err(observe_error(
                "durable audit principal binding request/session ids must be non-zero when present",
            ));
        }

        Ok(())
    }
}
