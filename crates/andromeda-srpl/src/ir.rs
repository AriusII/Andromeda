//! Compatibility facade for the canonical SRPL IR crate.
//!
//! `andromeda-srpl-ir` owns the IR model and validation. The historical
//! `andromeda-srpl::ir` module remains as a re-export boundary so existing
//! facade callers do not need to duplicate or depend on local shadow types.

pub use andromeda_srpl_ir::*;
