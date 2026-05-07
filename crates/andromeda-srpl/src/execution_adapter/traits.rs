use super::{
    contracts::{
        SrplAssertRequest, SrplEmitRequest, SrplFailureRequest, SrplReadRequest, SrplUpdateRequest,
    },
    diagnostics::SrplExecutionFailure,
    environment::SrplBindingEnvironment,
    results::{SrplAssertResult, SrplEmitResult, SrplReadResult, SrplUpdateResult},
};

/// Adapter trait for bounded typed reads/scans.
///
/// The read adapter receives a binding context that includes:
/// - Input parameter values from procedure inputs.
/// - Previously bound read results from prior read operations.
/// - Local variable bindings.
///
/// Predicates in the request are evaluated against this environment to filter rows.
pub trait SrplTypedReadAdapter {
    type Row;

    fn read_typed(
        &mut self,
        request: SrplReadRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplReadResult<Self::Row>, SrplExecutionFailure>;
}

/// Adapter trait for lowered SRPL assertion predicates.
///
/// The assertion adapter receives a binding context to evaluate predicates
/// against the current runtime state.
pub trait SrplAssertionAdapter {
    fn assert_typed(
        &mut self,
        request: SrplAssertRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplAssertResult, SrplExecutionFailure>;
}

/// Adapter trait for bounded typed updates.
///
/// The update adapter receives a binding context to validate that predicates can
/// bind to the environment and to preserve deterministic all-or-nothing behavior.
pub trait SrplTypedUpdateAdapter {
    fn update_typed(
        &mut self,
        request: SrplUpdateRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplUpdateResult, SrplExecutionFailure>;
}

/// Adapter trait for bounded typed result emission.
///
/// The emit adapter receives a binding context for consistency, though it
/// primarily uses the context for diagnostic and tracing purposes.
pub trait SrplTypedEmitAdapter {
    fn emit_typed(
        &mut self,
        request: SrplEmitRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplEmitResult, SrplExecutionFailure>;
}

/// Adapter trait for reporting typed SRPL semantic/execution failures.
///
/// The failure adapter receives a binding context for recovery purposes.
pub trait SrplFailureAdapter {
    fn fail_typed(
        &mut self,
        request: SrplFailureRequest,
        environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure>;
}
