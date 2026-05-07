//! SRPL procedure resolver contract.
//!
//! This module defines the pre-transaction boundary for resolving a procedure
//! identity plus contract expectation into the SRPL artifacts needed for a
//! later invocation.  It intentionally does **not** create transactions,
//! dispatch to a runtime, open storage, speak transport, or accept ad hoc query
//! text.  Implementors are catalog-facing adapters that must either return a
//! fully validated [`ProcedureResolveResponse`] or a typed
//! [`ProcedureResolveError`] before any transaction is created.
//!
//! ## Procedure contract checklist
//!
//! - **Inputs:** [`ProcedureResolveRequest`] carries a target
//!   ([`ProcedureId`] or [`QualifiedName`]), expected [`ContractHash`], and
//!   expected [`CatalogVersion`].  [`SrplProcedureManifest::inputs`] carries the
//!   scalar input columns required by the resolved contract.
//! - **StructuredObjects:** [`SrplProcedureManifest::structured_inputs`] carries
//!   typed catalog names for structured input objects.  The resolver only
//!   reports these names; it does not dereference storage.
//! - **Result streams:** [`SrplProcedureManifest::result_streams`] carries the
//!   catalog result stream contracts that callers must bind before payload
//!   handling.
//! - **Errors:** unknown procedure, contract hash mismatch, catalog version
//!   mismatch, invalid request/response, and resolver rejection are typed as
//!   [`ProcedureResolveError`].  No variant authorizes transaction creation.
//! - **Transaction policy:** [`SrplProcedureManifest::transaction_policy`]
//!   exposes the catalog policy that the transaction layer must later honor.
//!   This module never starts the transaction itself.
//! - **ContractHash inputs:** request validation rejects zero hashes; response
//!   validation requires the response hash, [`ProcedureContractRef`], manifest,
//!   and executable SRPL plan evidence to agree exactly with the request.

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
