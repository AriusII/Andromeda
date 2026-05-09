pub use andromeda_recovery::{
    ObservedBoundary, StartupAcceptance, StartupDecision, StartupEvidence, StartupOutcome,
    StartupRejectionReason, decide_startup,
};

pub type StartupAuditProjection =
    andromeda_recovery::StartupAuditProjection<andromeda_observe::TraceId>;
