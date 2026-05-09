//! SRPL catalog definition lifecycle compatibility tests.
//!
//! Gate prefixes follow the DefinitionBatch compatibility matrix:
//! `a*` covers contract shape/cardinality, `f*` covers staged compiler
//! failures, and `g*` covers DefinitionBatch dry-run/apply behavior.

#[path = "definitionbatch_compat/cardinality.rs"]
mod cardinality;
#[path = "definitionbatch_compat/diagnostics.rs"]
mod diagnostics;
#[path = "definitionbatch_compat/dry_run.rs"]
mod dry_run;
#[path = "definitionbatch_compat/procedure_contract.rs"]
mod procedure_contract;
#[path = "definitionbatch_compat/source_digest.rs"]
mod source_digest;
#[path = "definitionbatch_compat/support.rs"]
mod support;
