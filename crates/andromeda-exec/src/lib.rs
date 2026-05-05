#![forbid(unsafe_code)]
#![allow(ambiguous_glob_imports)]

extern crate andromeda_proto;

mod admission;
mod business;
pub mod dispatch;
pub mod executor_bridge;
pub mod helpers;
mod invocation;
mod local;
mod registry;
mod result;
pub mod services;
mod srpl_dispatch;
mod surface_gate;
mod vertical_slice_entry;
mod wal_evidence;

pub use admission::*;
pub use business::*;
pub use dispatch::{
    LocalDispatchPlan, LocalDispatchReceipt, LocalDispatcher, LocalRollbackPlan,
    LocalRollbackReceipt, PreTransactionDispatchEvidence, ProcedureDispatchRequest,
    ProcedureDispatchUnavailableReason, ProcedureDispatcher, RemoteProcedureDispatcherUnavailable,
    RollbackCause, RollbackWalDurabilityEvidence, WalDurabilityEvidence,
};
pub use executor_bridge::ExecutorDispatchBridge;
#[allow(deprecated)]
pub use helpers::transaction_id_for_invocation;
pub use invocation::*;
pub use local::*;
pub use registry::{ProcedureHandler, ProcedureRegistry, ReserveStockProcedureHandler};
pub use result::*;
pub use services::{
    AdmissionService, CompletionMappingService, PreTransactionValidationService,
    ResultValidationService,
};
pub use srpl_dispatch::SrplProcedureDispatcher;
pub use surface_gate::{
    AuthorizedProcedureDispatch, SurfacePlaneAuthorizer, surface_plane_to_scope,
};
pub use vertical_slice_entry::*;
pub use wal_evidence::*;
