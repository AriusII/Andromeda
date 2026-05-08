//! Catalog-facing ScenarioEvidence model.
//!
//! This module owns the canonical advisory evidence primitives. The catalog
//! crate reexports these types as a compatibility facade so existing
//! `andromeda_catalog::ScenarioEvidence` imports remain stable.

mod advisory;
mod digest;
mod errors;
mod evidence;
mod identity;
mod score;
mod validity;

#[cfg(test)]
mod tests;

pub use advisory::{ScenarioEvidenceAdvisoryUse, ScenarioEvidenceOptimizerBoundary};
pub use errors::ScenarioEvidenceError;
pub use evidence::ScenarioEvidence;
pub use identity::{ScenarioId, ScenarioKind, ScenarioTarget};
pub use score::{EvidenceConfidence, EvidenceScore};
pub use validity::ValidityWindow;
