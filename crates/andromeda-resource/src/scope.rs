use std::fmt;

use andromeda_types::ProcedureId;

use crate::{ResourceScopeError, ResourceScopeResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceBudgetScopeKind {
    Procedure,
    Job,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceJobName(String);

impl ResourceJobName {
    pub fn new(name: impl Into<String>) -> ResourceScopeResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ResourceScopeError::EmptyJobName);
        }

        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceJobName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceBudgetScope {
    Procedure { procedure_id: ProcedureId },
    Job { job: ResourceJobName },
}

impl ResourceBudgetScope {
    pub const fn for_procedure(procedure_id: ProcedureId) -> Self {
        Self::Procedure { procedure_id }
    }

    pub fn for_job(job: impl Into<String>) -> ResourceScopeResult<Self> {
        Ok(Self::Job {
            job: ResourceJobName::new(job)?,
        })
    }

    pub const fn kind(&self) -> ResourceBudgetScopeKind {
        match self {
            Self::Procedure { .. } => ResourceBudgetScopeKind::Procedure,
            Self::Job { .. } => ResourceBudgetScopeKind::Job,
        }
    }
}

impl fmt::Display for ResourceBudgetScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Procedure { procedure_id } => write!(f, "ProcedureId {}", procedure_id.get()),
            Self::Job { job } => write!(f, "job {}", job),
        }
    }
}

#[cfg(test)]
mod tests {
    use andromeda_types::ProcedureId;

    use super::*;

    #[test]
    fn procedure_scope_preserves_typed_identity() {
        let scope = ResourceBudgetScope::for_procedure(ProcedureId::new(42));

        assert_eq!(scope.kind(), ResourceBudgetScopeKind::Procedure);
        assert_eq!(scope.to_string(), "ProcedureId 42");
    }

    #[test]
    fn job_scope_rejects_blank_names() {
        assert_eq!(
            ResourceBudgetScope::for_job("   ").unwrap_err(),
            ResourceScopeError::EmptyJobName
        );
    }
}
