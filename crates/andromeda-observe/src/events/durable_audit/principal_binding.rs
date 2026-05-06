use andromeda_core::{AndromedaResult, RequestId, SessionId};

use crate::events::{Permission, SurfaceScope, contains_sensitive_marker, observe_error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableAuditPrincipalBinding {
    pub principal_id: String,
    pub certificate_fingerprint: Option<String>,
    pub surface: Option<SurfaceScope>,
    pub permission: Option<Permission>,
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
        {
            return Err(observe_error(
                "durable audit principal binding must not contain secret evidence",
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
