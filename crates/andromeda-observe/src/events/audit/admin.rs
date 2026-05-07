use andromeda_error::AndromedaResult;

use crate::TraceId;

use super::super::EventSchemaVersion;
use super::{
    AdminOperation, CertificateIdentity, Permission, SurfaceScope, UserPrincipal,
    contains_sensitive_marker, non_empty_reason,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminOperationTrace {
    pub trace_id: TraceId,
    pub schema_version: EventSchemaVersion,
    pub surface: SurfaceScope,
    pub certificate: CertificateIdentity,
    pub principal: UserPrincipal,
    pub operation: AdminOperation,
    pub permission: Permission,
    pub accepted: bool,
    pub reason: String,
}

impl AdminOperationTrace {
    #[allow(
        clippy::too_many_arguments,
        reason = "Audit trace construction keeps certificate, principal, permission, and decision fields explicit."
    )]
    pub fn new(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        operation: AdminOperation,
        permission: Permission,
        accepted: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            trace_id,
            schema_version: EventSchemaVersion::V0,
            surface,
            certificate,
            principal,
            operation,
            permission,
            accepted,
            reason: non_empty_reason(reason)?,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_supported_schema_version(&self) -> bool {
        self.schema_version.is_v0()
    }

    pub const fn surface_permits_operation(&self) -> bool {
        self.surface
            .permits_permission(self.operation.required_permission())
    }

    pub fn has_identity_evidence(&self) -> bool {
        self.certificate.has_identity_evidence() && self.principal.has_identity_evidence()
    }

    pub const fn surface_matches_certificate(&self) -> bool {
        self.surface as u8 == self.certificate.surface as u8
    }

    pub const fn permission_matches_operation(&self) -> bool {
        self.permission.authorizes_admin_operation(self.operation)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        self.certificate.contains_sensitive_evidence()
            || self.principal.contains_sensitive_evidence()
            || contains_sensitive_marker(&self.reason)
    }
}
