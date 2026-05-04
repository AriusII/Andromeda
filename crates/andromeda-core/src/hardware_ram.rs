//! RAM memory budget allocation and validation.
//!
//! This module defines how memory is partitioned into functional sections
//! and provides validation for budget consistency.

use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Logical role for a RAM section in the execution environment.
///
/// Used to categorize memory allocations by their purpose, enabling
/// different performance and reliability policies per section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSectionRole {
    /// Memory for catalog objects and metadata
    Catalog,
    /// Memory for active query execution
    Execution,
    /// Memory for caching and hot data
    Cache,
    /// Memory for temporary working set (spill buffers, sort workspace)
    Temp,
    /// Memory for I/O buffering
    Io,
}

/// Budget allocation for a specific RAM section.
///
/// Specifies the maximum bytes available for a particular functional role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamSectionBudget {
    /// The role this budget applies to
    pub role: RamSectionRole,
    /// Maximum bytes for this section
    pub max_bytes: u64,
}

impl RamSectionBudget {
    /// Creates a new RAM section budget.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let budget = RamSectionBudget::new(RamSectionRole::Execution, 1_000_000);
    /// ```
    pub const fn new(role: RamSectionRole, max_bytes: u64) -> Self {
        Self { role, max_bytes }
    }
}

/// RAM profile describing total memory and section allocations.
///
/// Validates that per-section budgets do not exceed the declared total,
/// ensuring consistency of memory constraints throughout the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamProfile {
    /// Total bytes available across all sections
    pub total_bytes: u64,
    /// Individual budgets by functional role
    pub sections: Vec<RamSectionBudget>,
}

impl RamProfile {
    /// Creates a conservative RAM profile with no memory.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let profile = RamProfile::conservative();
    /// ```
    pub const fn conservative() -> Self {
        Self {
            total_bytes: 0,
            sections: Vec::new(),
        }
    }

    /// Creates a RAM profile with explicit total and section budgets.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let sections = vec![
    ///     RamSectionBudget::new(RamSectionRole::Execution, 64),
    /// ];
    /// let profile = RamProfile::new(128, sections);
    /// ```
    pub fn new(total_bytes: u64, sections: Vec<RamSectionBudget>) -> Self {
        Self {
            total_bytes,
            sections,
        }
    }

    /// Returns the budget in bytes for a specific section role, if present.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(profile.section_budget_bytes(RamSectionRole::Execution), Some(64));
    /// ```
    pub fn section_budget_bytes(&self, role: RamSectionRole) -> Option<u64> {
        self.sections
            .iter()
            .find(|section| section.role == role)
            .map(|section| section.max_bytes)
    }

    /// Returns the sum of all declared section budgets.
    ///
    /// Uses saturating arithmetic to avoid overflow.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let declared = profile.declared_section_bytes();
    /// ```
    pub fn declared_section_bytes(&self) -> u64 {
        self.sections
            .iter()
            .map(|section| section.max_bytes)
            .fold(0_u64, u64::saturating_add)
    }

    /// Validates that section budgets do not exceed the total declared budget.
    ///
    /// # Errors
    ///
    /// Returns an error if the sum of section budgets exceeds `total_bytes`
    /// when a nonzero total was declared.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// profile.validate_budgets()?;
    /// ```
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
