//! Permission scope validation for Procedure dispatch.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{DurableAuditSinkReport, TraceId};
use std::collections::HashSet;

use crate::services::permission_audit_emitter::{
    AuditEmissionEvidence, AuditEmissionPolicy, AuditSinkAvailability,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionScopeValidation {
    Allowed,
    Denied { unauthorized_permission: String },
}

impl PermissionScopeValidation {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    pub fn into_error(self) -> Option<AndromedaError> {
        match self {
            Self::Allowed => None,
            Self::Denied {
                unauthorized_permission,
            } => Some(AndromedaError::new(
                AndromedaErrorKind::Security,
                format!(
                    "requested permission \"{}\" exceeds handler contract scope; permission escalation prevented",
                    unauthorized_permission
                ),
            )),
        }
    }
}

pub fn validate_dispatch_permissions(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
) -> PermissionScopeValidation {
    if request_permissions.is_empty() {
        return PermissionScopeValidation::Allowed;
    }

    if handler_contract_permissions.is_empty() {
        let unauthorized_permission = match request_permissions.first() {
            Some(permission) => permission.clone(),
            None => return PermissionScopeValidation::Allowed,
        };
        return PermissionScopeValidation::Denied {
            unauthorized_permission,
        };
    }

    let handler_set: HashSet<&str> = handler_contract_permissions
        .iter()
        .map(|p| p.as_str())
        .collect();

    for request_perm in request_permissions {
        if !handler_set.contains(request_perm.as_str()) {
            return PermissionScopeValidation::Denied {
                unauthorized_permission: request_perm.clone(),
            };
        }
    }

    PermissionScopeValidation::Allowed
}

pub fn validate_dispatch_permissions_or_error(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
) -> AndromedaResult<()> {
    let validation =
        validate_dispatch_permissions(request_permissions, handler_contract_permissions);
    match validation.into_error() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn validate_dispatch_permissions_with_audit(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
    trace_id: TraceId,
    policy: AuditEmissionPolicy,
    sink: AuditSinkAvailability,
) -> AndromedaResult<(PermissionScopeValidation, Option<AuditEmissionEvidence>)> {
    let validation =
        validate_dispatch_permissions(request_permissions, handler_contract_permissions);
    let audit = match &validation {
        PermissionScopeValidation::Allowed => None,
        PermissionScopeValidation::Denied {
            unauthorized_permission,
        } => Some(AuditEmissionEvidence::contract_rejected(
            policy,
            trace_id,
            format!(
                "dispatch permission exceeds handler contract scope: {unauthorized_permission}"
            ),
            sink,
        )?),
    };

    Ok((validation, audit))
}

pub fn validate_dispatch_permissions_with_durable_audit(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
    trace_id: TraceId,
    policy: AuditEmissionPolicy,
    report: DurableAuditSinkReport,
) -> AndromedaResult<(PermissionScopeValidation, Option<AuditEmissionEvidence>)> {
    let validation =
        validate_dispatch_permissions(request_permissions, handler_contract_permissions);
    let PermissionScopeValidation::Denied {
        unauthorized_permission,
    } = &validation
    else {
        return Ok((validation, None));
    };

    let sink = AuditSinkAvailability::durable_for_policy(policy, report)?;
    let audit = AuditEmissionEvidence::contract_rejected(
        policy,
        trace_id,
        format!("dispatch permission exceeds handler contract scope: {unauthorized_permission}"),
        sink,
    )?;

    Ok((validation, Some(audit)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_request_permissions_is_allowed() {
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&[], &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_all_permissions_in_scope_is_allowed() {
        let request = vec!["inventory.reserve".to_string()];
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_subset_of_permissions_is_allowed() {
        let request = vec!["inventory.reserve".to_string()];
        let handler = vec![
            "inventory.reserve".to_string(),
            "inventory.query".to_string(),
            "inventory.release".to_string(),
        ];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_multiple_permissions_all_in_scope_is_allowed() {
        let request = vec![
            "inventory.reserve".to_string(),
            "inventory.query".to_string(),
        ];
        let handler = vec![
            "inventory.reserve".to_string(),
            "inventory.query".to_string(),
            "inventory.release".to_string(),
        ];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_exact_match_is_allowed() {
        let perms = vec![
            "inventory.reserve".to_string(),
            "inventory.query".to_string(),
        ];
        let result = validate_dispatch_permissions(&perms, &perms);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_with_many_permissions_all_in_scope_is_allowed() {
        let request = vec![
            "perm1".to_string(),
            "perm2".to_string(),
            "perm3".to_string(),
        ];
        let handler = vec![
            "perm0".to_string(),
            "perm1".to_string(),
            "perm2".to_string(),
            "perm3".to_string(),
            "perm4".to_string(),
        ];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_request_permission_not_in_handler_is_denied() {
        let request = vec!["inventory.reserve".to_string()];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(
            result,
            PermissionScopeValidation::Denied {
                unauthorized_permission: "inventory.reserve".to_string()
            }
        );
    }

    #[test]
    fn validate_empty_handler_permissions_denies_any_request() {
        let request = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &[]);
        assert_eq!(
            result,
            PermissionScopeValidation::Denied {
                unauthorized_permission: "inventory.reserve".to_string()
            }
        );
    }

    #[test]
    fn validate_one_permission_out_of_scope_is_denied() {
        let request = vec![
            "inventory.reserve".to_string(),
            "inventory.query".to_string(),
            "inventory.admin".to_string(),
        ];
        let handler = vec![
            "inventory.reserve".to_string(),
            "inventory.query".to_string(),
        ];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(
            result,
            PermissionScopeValidation::Denied {
                unauthorized_permission: "inventory.admin".to_string()
            }
        );
    }

    #[test]
    fn validate_privilege_escalation_attempt_first_permission_is_denied() {
        let request = vec!["admin.global".to_string()];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(
            result,
            PermissionScopeValidation::Denied {
                unauthorized_permission: "admin.global".to_string()
            }
        );
    }

    #[test]
    fn validate_privilege_escalation_attempt_middle_permission_is_denied() {
        let request = vec![
            "inventory.query".to_string(),
            "admin.global".to_string(),
            "inventory.reserve".to_string(),
        ];
        let handler = vec![
            "inventory.query".to_string(),
            "inventory.reserve".to_string(),
        ];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(
            result,
            PermissionScopeValidation::Denied {
                unauthorized_permission: "admin.global".to_string()
            }
        );
    }

    #[test]
    fn validate_case_sensitive_permissions() {
        let request = vec!["Inventory.Reserve".to_string()];
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.is_denied());
    }

    #[test]
    fn validate_whitespace_matters_in_permissions() {
        let request = vec!["inventory.reserve ".to_string()];
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.is_denied());
    }

    #[test]
    fn validate_allowed_produces_no_error() {
        let request = vec!["inventory.reserve".to_string()];
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.into_error().is_none());
    }

    #[test]
    fn validate_denied_produces_error_with_permission_name() -> AndromedaResult<()> {
        let request = vec!["admin.global".to_string()];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        let Some(error) = result.into_error() else {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "permission denial should produce an error",
            ));
        };
        assert!(error.to_string().contains("admin.global"));
        assert!(error.to_string().contains("escalation"));
        Ok(())
    }

    #[test]
    fn validate_dispatch_permissions_or_error_succeeds_on_valid() {
        let request = vec!["inventory.reserve".to_string()];
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions_or_error(&request, &handler);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_dispatch_permissions_or_error_fails_on_invalid() {
        let request = vec!["admin.global".to_string()];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions_or_error(&request, &handler);
        assert!(result.is_err());
    }

    #[test]
    fn validate_dispatch_permissions_or_error_empty_request_succeeds() {
        let result = validate_dispatch_permissions_or_error(&[], &["inventory.query".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_dispatch_permissions_or_error_empty_handler_fails() {
        let result = validate_dispatch_permissions_or_error(&["inventory.query".to_string()], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_handles_deeply_nested_permission_names() {
        let request = vec!["a.b.c.d.e.f".to_string()];
        let handler = vec!["a.b.c.d.e.f".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_denies_similar_but_different_nested_permissions() {
        let request = vec!["a.b.c.d.e.g".to_string()];
        let handler = vec!["a.b.c.d.e.f".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.is_denied());
    }

    #[test]
    fn validate_with_special_characters_in_permission_names() {
        let request = vec!["namespace:resource:action".to_string()];
        let handler = vec!["namespace:resource:action".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_denies_if_special_chars_dont_match() {
        let request = vec!["namespace:resource:action".to_string()];
        let handler = vec!["namespace|resource|action".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.is_denied());
    }

    #[test]
    fn validate_many_permissions_in_handler_finds_match() {
        let mut handler = Vec::new();
        for i in 0..1000 {
            handler.push(format!("perm_{}", i));
        }
        let request = vec!["perm_500".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_many_permissions_in_handler_denies_missing() {
        let mut handler = Vec::new();
        for i in 0..1000 {
            handler.push(format!("perm_{}", i));
        }
        let request = vec!["perm_2000".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.is_denied());
    }

    #[test]
    fn validate_many_request_permissions_all_in_scope() {
        let mut request = Vec::new();
        let mut handler = Vec::new();
        for i in 0..100 {
            request.push(format!("perm_{}", i));
            handler.push(format!("perm_{}", i));
        }
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_duplicates_in_request_and_handler_allowed() {
        let request = vec!["inventory.query".to_string(), "inventory.query".to_string()];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }
}
