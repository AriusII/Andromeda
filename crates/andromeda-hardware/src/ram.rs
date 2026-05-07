//! RAM budget allocation and validation.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Logical role for a RAM section in the execution environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSectionRole {
    Catalog,
    Execution,
    Cache,
    Temp,
    Io,
}

/// Budget allocation for a specific RAM section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamSectionBudget {
    pub role: RamSectionRole,
    pub max_bytes: u64,
}

impl RamSectionBudget {
    pub const fn new(role: RamSectionRole, max_bytes: u64) -> Self {
        Self { role, max_bytes }
    }
}

/// RAM profile describing total memory and section allocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamProfile {
    pub total_bytes: u64,
    pub sections: Vec<RamSectionBudget>,
}

impl RamProfile {
    pub const fn conservative() -> Self {
        Self {
            total_bytes: 0,
            sections: Vec::new(),
        }
    }

    pub fn new(total_bytes: u64, sections: Vec<RamSectionBudget>) -> Self {
        Self {
            total_bytes,
            sections,
        }
    }

    pub fn section_budget_bytes(&self, role: RamSectionRole) -> Option<u64> {
        self.sections
            .iter()
            .find(|section| section.role == role)
            .map(|section| section.max_bytes)
    }

    /// Uses saturating arithmetic to avoid overflow.
    pub fn declared_section_bytes(&self) -> u64 {
        self.sections
            .iter()
            .map(|section| section.max_bytes)
            .fold(0_u64, u64::saturating_add)
    }

    /// Rejects section budgets that exceed a nonzero declared total.
    pub fn validate_budgets(&self) -> AndromedaResult<()> {
        let declared = self.declared_section_bytes();
        if self.total_bytes != 0 && declared > self.total_bytes {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "RAM section budgets exceed declared total bytes",
            ));
        }

        Ok(())
    }
}

impl Default for RamProfile {
    fn default() -> Self {
        Self::conservative()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_profile_validates_section_budgets() {
        let profile = RamProfile::new(
            128,
            vec![
                RamSectionBudget::new(RamSectionRole::Execution, 64),
                RamSectionBudget::new(RamSectionRole::Temp, 32),
            ],
        );

        assert_eq!(
            profile.section_budget_bytes(RamSectionRole::Execution),
            Some(64)
        );
        assert_eq!(profile.section_budget_bytes(RamSectionRole::Cache), None);
        assert!(profile.validate_budgets().is_ok());

        let over_budget = RamProfile::new(
            8,
            vec![
                RamSectionBudget::new(RamSectionRole::Execution, 8),
                RamSectionBudget::new(RamSectionRole::Temp, 8),
            ],
        );

        assert!(over_budget.validate_budgets().is_err());
    }

    #[test]
    fn declared_section_bytes_uses_saturating_add() {
        let profile = RamProfile::new(
            u64::MAX,
            vec![
                RamSectionBudget::new(RamSectionRole::Execution, u64::MAX),
                RamSectionBudget::new(RamSectionRole::Temp, u64::MAX),
            ],
        );

        assert_eq!(profile.declared_section_bytes(), u64::MAX);
    }
}
