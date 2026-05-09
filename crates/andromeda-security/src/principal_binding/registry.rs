use std::collections::BTreeMap;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::PrincipalBinding;

/// In-memory registry of certificate-fingerprint to [`PrincipalBinding`].
#[derive(Debug, Default, Clone)]
pub struct PrincipalRegistry {
    bindings: BTreeMap<String, PrincipalBinding>,
}

impl PrincipalRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
        }
    }

    /// Register a binding, indexed by the certificate fingerprint.
    /// Re-registering the same fingerprint with a different principal id is
    /// rejected to make rotation an explicit, observable workflow.
    pub fn register(&mut self, binding: PrincipalBinding) -> AndromedaResult<()> {
        let key = binding.certificate.fingerprint.clone();
        if let Some(existing) = self.bindings.get(&key)
            && existing.principal.principal_id != binding.principal.principal_id
        {
            return Err(security_error(
                "certificate fingerprint already bound to a different principal id; explicit rotation required",
            ));
        }
        self.bindings.insert(key, binding);
        Ok(())
    }

    /// Number of registered bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Look up a binding by certificate fingerprint.
    pub fn lookup(&self, fingerprint: &str) -> Option<&PrincipalBinding> {
        self.bindings.get(fingerprint)
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}
