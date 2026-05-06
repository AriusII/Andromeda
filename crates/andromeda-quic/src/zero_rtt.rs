use crate::EarlyDataPolicy;

/// Request classes considered by the runtime-free QUIC 0-RTT doctrine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZeroRttReplayClass {
    /// Procedure may mutate durable or externally visible state.
    MutatingProcedure,
    /// Procedure idempotency is not known at the transport boundary.
    UnknownIdempotency,
    /// Authentication or authorization state may change.
    AuthChangingOperation,
    /// Catalog procedures are never admitted through early data in V1.
    CatalogProcedure,
    /// HA/DR promotion changes cluster leadership or authority.
    HadrPromotion,
    /// HA/DR demotion changes cluster leadership or authority.
    HadrDemotion,
    /// Manifest reads are replay-safe but still barred by the V1 doctrine.
    ReadOnlyManifest,
    /// Telemetry reads are replay-safe but still barred by the V1 doctrine.
    ReadOnlyTelemetry,
}

impl ZeroRttReplayClass {
    /// Returns true when the request class is intrinsically replay-safe.
    pub const fn is_replay_safe(self) -> bool {
        matches!(self, Self::ReadOnlyManifest | Self::ReadOnlyTelemetry)
    }
}

/// Pure 0-RTT admission policy shared by runtime adapters and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZeroRttAdmissionPolicy {
    early_data: EarlyDataPolicy,
}

impl ZeroRttAdmissionPolicy {
    /// Canonical V1 doctrine: classify requests, then reject all early data.
    pub const fn doctrine_v1_disabled() -> Self {
        Self {
            early_data: EarlyDataPolicy::Disabled,
        }
    }

    /// Builds a 0-RTT admission policy from the listener early-data mode.
    pub const fn from_early_data_policy(early_data: EarlyDataPolicy) -> Self {
        Self { early_data }
    }

    /// Returns the global listener early-data mode backing this policy.
    pub const fn early_data_policy(self) -> EarlyDataPolicy {
        self.early_data
    }

    /// Evaluates QUIC 0-RTT admission without touching a runtime or socket.
    pub const fn evaluate(self, class: ZeroRttReplayClass) -> ZeroRttAdmissionDecision {
        let reason = match class {
            ZeroRttReplayClass::MutatingProcedure => {
                ZeroRttAdmissionRejectionReason::MutatingProcedure
            }
            ZeroRttReplayClass::UnknownIdempotency => {
                ZeroRttAdmissionRejectionReason::UnknownIdempotency
            }
            ZeroRttReplayClass::AuthChangingOperation => {
                ZeroRttAdmissionRejectionReason::AuthChangingOperation
            }
            ZeroRttReplayClass::CatalogProcedure => {
                ZeroRttAdmissionRejectionReason::CatalogProcedure
            }
            ZeroRttReplayClass::HadrPromotion => ZeroRttAdmissionRejectionReason::HadrPromotion,
            ZeroRttReplayClass::HadrDemotion => ZeroRttAdmissionRejectionReason::HadrDemotion,
            ZeroRttReplayClass::ReadOnlyManifest | ZeroRttReplayClass::ReadOnlyTelemetry => {
                match self.early_data {
                    EarlyDataPolicy::Disabled => {
                        ZeroRttAdmissionRejectionReason::DoctrineV1DisablesEarlyData
                    }
                }
            }
        };

        ZeroRttAdmissionDecision::Reject { class, reason }
    }
}

impl Default for ZeroRttAdmissionPolicy {
    fn default() -> Self {
        Self::doctrine_v1_disabled()
    }
}

/// Runtime-free admission decision for a classified 0-RTT request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZeroRttAdmissionDecision {
    /// 0-RTT is admitted for the classified request.
    Admit { class: ZeroRttReplayClass },
    /// 0-RTT is rejected with a typed policy reason.
    Reject {
        class: ZeroRttReplayClass,
        reason: ZeroRttAdmissionRejectionReason,
    },
}

impl ZeroRttAdmissionDecision {
    /// Returns true when early data is admitted.
    pub const fn is_admitted(self) -> bool {
        matches!(self, Self::Admit { .. })
    }

    /// Returns the classified request used to make this decision.
    pub const fn class(self) -> ZeroRttReplayClass {
        match self {
            Self::Admit { class } | Self::Reject { class, .. } => class,
        }
    }

    /// Returns the rejection reason, when early data is rejected.
    pub const fn rejection_reason(self) -> Option<ZeroRttAdmissionRejectionReason> {
        match self {
            Self::Admit { .. } => None,
            Self::Reject { reason, .. } => Some(reason),
        }
    }
}

/// Typed 0-RTT rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZeroRttAdmissionRejectionReason {
    MutatingProcedure,
    UnknownIdempotency,
    AuthChangingOperation,
    CatalogProcedure,
    HadrPromotion,
    HadrDemotion,
    DoctrineV1DisablesEarlyData,
}
