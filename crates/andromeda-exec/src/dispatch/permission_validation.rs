//! Permission scope validation for Procedure dispatch.
//!
//! This module enforces the security invariant that caller-supplied permissions
//! must be a subset of handler contract permissions, preventing privilege escalation.
//!
//! ## Security Model
//!
//! **Invariant:** `∀ request_perm ∈ request_permissions ⊆ handler_contract_permissions`
//!
//! Every dispatched Procedure must satisfy:
//! - Requested permissions are explicitly allowed by the handler contract
//! - No permission escalation is possible
//! - Denials are fail-safe and auditable
//!
//! ## Validation Strategy
//!
//! For each permission in the request:
//! 1. Check that it exists in the handler's contract permissions
//! 2. If any permission is missing, deny the invocation
//! 3. Return clear, auditable reason for denial
//!
//! ## Future Evolution (Wave 21+)
//!
//! - Scope filtering (namespace-scoped permissions)
//! - Role hierarchy with transitive permission expansion
//! - ABAC with runtime attribute predicates
//! - Permission caching with TTL

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::HashSet;

/// Result of permission scope validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionScopeValidation {
    /// All requested permissions are within scope of handler contract permissions.
    Allowed,
    /// At least one requested permission exceeds handler scope.
    Denied {
        /// Permission that was requested but not in handler scope.
        unauthorized_permission: String,
    },
}

impl PermissionScopeValidation {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    pub fn as_error(self) -> Option<AndromedaError> {
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

/// Validate that all request permissions are within handler contract scope.
///
/// This function enforces the security invariant that prevents privilege escalation:
/// `∀ request_perm ∈ request_permissions ⊆ handler_permissions`
///
/// ## Arguments
///
/// * `request_permissions` - Permissions claimed by the invocation requester
/// * `handler_contract_permissions` - Permissions the handler contract allows
///
/// ## Returns
///
/// - `Ok(PermissionScopeValidation::Allowed)` if all request permissions are in scope
/// - `Ok(PermissionScopeValidation::Denied { unauthorized_permission })` if any permission exceeds scope
/// - The function always succeeds (never returns `Err`), returning validation results as an enum
pub fn validate_dispatch_permissions(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
) -> PermissionScopeValidation {
    // Empty request permissions are always valid (principle of least privilege)
    if request_permissions.is_empty() {
        return PermissionScopeValidation::Allowed;
    }

    // Empty handler permissions deny all non-empty requests (fail-safe default)
    if handler_contract_permissions.is_empty() {
        return PermissionScopeValidation::Denied {
            unauthorized_permission: request_permissions
                .first()
                .map(|p| p.clone())
                .unwrap_or_default(),
        };
    }

    // Convert handler permissions to a set for O(1) lookup
    let handler_set: HashSet<&str> = handler_contract_permissions
        .iter()
        .map(|p| p.as_str())
        .collect();

    // Check each request permission is in handler scope
    for request_perm in request_permissions {
        if !handler_set.contains(request_perm.as_str()) {
            return PermissionScopeValidation::Denied {
                unauthorized_permission: request_perm.clone(),
            };
        }
    }

    PermissionScopeValidation::Allowed
}

/// Validate dispatch permissions and return an error if validation fails.
///
/// This is a convenience wrapper that converts `PermissionScopeValidation` to `AndromedaResult`.
pub fn validate_dispatch_permissions_or_error(
    request_permissions: &[String],
    handler_contract_permissions: &[String],
) -> AndromedaResult<()> {
    let validation =
        validate_dispatch_permissions(request_permissions, handler_contract_permissions);
    match validation.as_error() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Allowed Cases ---

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

    // --- Denied Cases ---

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
            "inventory.admin".to_string(), // This one is unauthorized
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
            "admin.global".to_string(), // Escalation attempt
            "inventory.reserve".to_string(),
        ];
        let handler = vec![
            "inventory.query".to_string(),
            "inventory.reserve".to_string(),
        ];
        let result = validate_dispatch_permissions(&request, &handler);
        // Should detect the escalation attempt
        assert!(result.is_denied());
        if let PermissionScopeValidation::Denied {
            unauthorized_permission,
        } = result
        {
            assert_eq!(unauthorized_permission, "admin.global");
        } else {
            panic!("Expected denied result");
        }
    }

    #[test]
    fn validate_case_sensitive_permissions() {
        let request = vec!["Inventory.Reserve".to_string()]; // Capital I and R
        let handler = vec!["inventory.reserve".to_string()]; // lowercase
        let result = validate_dispatch_permissions(&request, &handler);
        // Should be denied because permissions are case-sensitive
        assert!(result.is_denied());
    }

    #[test]
    fn validate_whitespace_matters_in_permissions() {
        let request = vec!["inventory.reserve ".to_string()]; // trailing space
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        // Should be denied because of whitespace difference
        assert!(result.is_denied());
    }

    // --- Error Conversion ---

    #[test]
    fn validate_allowed_produces_no_error() {
        let request = vec!["inventory.reserve".to_string()];
        let handler = vec!["inventory.reserve".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.as_error().is_none());
    }

    #[test]
    fn validate_denied_produces_error_with_permission_name() {
        let request = vec!["admin.global".to_string()];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        let error = result.as_error().expect("Should produce error");
        assert!(error.to_string().contains("admin.global"));
        assert!(error.to_string().contains("escalation"));
    }

    // --- Wrapper Function Tests ---

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

    // --- Permission Structure Tests ---

    #[test]
    fn validate_handles_deeply_nested_permission_names() {
        let request = vec!["a.b.c.d.e.f".to_string()];
        let handler = vec!["a.b.c.d.e.f".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }

    #[test]
    fn validate_denies_similar_but_different_nested_permissions() {
        let request = vec!["a.b.c.d.e.g".to_string()]; // ends with 'g' not 'f'
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
        let handler = vec!["namespace|resource|action".to_string()]; // Different separators
        let result = validate_dispatch_permissions(&request, &handler);
        assert!(result.is_denied());
    }

    // --- Performance / Stress Tests ---

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
        let request = vec!["perm_2000".to_string()]; // Not in handler
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
        let request = vec![
            "inventory.query".to_string(),
            "inventory.query".to_string(), // Duplicate
        ];
        let handler = vec!["inventory.query".to_string()];
        let result = validate_dispatch_permissions(&request, &handler);
        assert_eq!(result, PermissionScopeValidation::Allowed);
    }
}
