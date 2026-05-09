use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_principal::{
    PrincipalAuthorizationDecision, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvidence,
};

/// Route admission failure with optional authorization evidence.
///
/// Transport, frame, protobuf, and catalog binding failures do not have IAM
/// evidence because no principal policy decision was reached. IAM denials carry
/// the core evidence needed for durable audit records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureRouteAdmissionError {
    error: AndromedaError,
    authorization_denial_reason: Option<PrincipalAuthorizationDenialReason>,
    authorization_evidence: Option<Box<PrincipalAuthorizationEvidence>>,
}

impl ProcedureRouteAdmissionError {
    pub(super) fn route(error: AndromedaError) -> Self {
        Self {
            error,
            authorization_denial_reason: None,
            authorization_evidence: None,
        }
    }

    pub(super) fn authorization(
        error: AndromedaError,
        authorization: PrincipalAuthorizationDecision,
    ) -> Self {
        Self {
            error,
            authorization_denial_reason: authorization.denial_reason,
            authorization_evidence: Some(Box::new(authorization.evidence)),
        }
    }

    pub const fn error(&self) -> &AndromedaError {
        &self.error
    }

    pub fn kind(&self) -> AndromedaErrorKind {
        self.error.kind()
    }

    pub fn message(&self) -> &str {
        self.error.message()
    }

    pub const fn authorization_denial_reason(&self) -> Option<PrincipalAuthorizationDenialReason> {
        self.authorization_denial_reason
    }

    pub fn authorization_evidence(&self) -> Option<&PrincipalAuthorizationEvidence> {
        self.authorization_evidence.as_deref()
    }
}

impl std::fmt::Display for ProcedureRouteAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for ProcedureRouteAdmissionError {}

pub(super) fn protocol_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

pub(super) fn security_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
