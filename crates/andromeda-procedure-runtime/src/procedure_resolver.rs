//! Procedure resolver contract for SRPL pre-dispatch resolution.
//!
//! This module defines the pre-transaction boundary for resolving a procedure
//! identity plus contract expectation into the SRPL artifacts needed for a
//! later invocation. It does not create transactions, dispatch to a runtime,
//! open storage, speak transport, or accept ad hoc query text. Implementors are
//! catalog-facing adapters that must either return a fully validated
//! [`ProcedureResolveResponse`] or a typed [`ProcedureResolveError`] before any
//! transaction is created.

mod error;
mod manifest;
mod request;
mod resolver;
mod response;
mod validation;

pub use error::ProcedureResolveError;
pub use manifest::SrplProcedureManifest;
pub use request::{ProcedureResolveRequest, ProcedureResolveTarget};
pub use resolver::ProcedureResolver;
pub use response::ProcedureResolveResponse;

#[cfg(test)]
mod tests;
