//! Backpressure signaling for flow control and resource management.
//!
//! Backpressure is a typed control concept: it carries a reason, an optional
//! request id, and a bounded retry delay. It is *sendable* on a constrained
//! set of transport surfaces - never on RPC command, RPC result, or session
//! control streams - and when carried over a telemetry datagram it must fit
//! within the negotiated MTU. These rules are enforced here by
//! [`BackpressureSignal::validate_routing`] and
//! [`BackpressureSignal::validate_for_transport`].

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::RequestId;

use crate::StreamRole;

/// Reason for backpressure signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureReason {
    /// Receive buffer is saturated.
    ReceiveBufferSaturated,
    /// Client is processing slowly.
    SlowClient,
    /// Execution queue is saturated.
    ExecutionQueueSaturated,
    /// WAL flush is lagging.
    WalFlushLag,
    /// Hot store pressure.
    HotStorePressure,
    /// Temporary store quota exceeded.
    TempStoreQuota,
    /// Result spool is growing.
    ResultSpoolGrowth,
    /// Catalog lock contention.
    CatalogLockContention,
}

/// Backpressure signal with retry guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureSignal {
    pub reason: BackpressureReason,
    pub request_id: Option<RequestId>,
    pub retry_after_millis: Option<u64>,
}

impl BackpressureReason {
    /// Returns true if this reason is scoped to a specific request.
    pub const fn is_request_scoped(self) -> bool {
        matches!(
            self,
            Self::SlowClient | Self::ExecutionQueueSaturated | Self::ResultSpoolGrowth
        )
    }
}

/// Transport surface a backpressure signal is being routed onto.
///
/// Backpressure is intentionally a low-cardinality control concept: it is
/// either propagated reliably on the diagnostic stream (where it can carry a
/// request id and survive loss) or fired and forgotten on a telemetry datagram
/// (where the encoded form must fit the negotiated MTU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureTransport {
    /// Reliable diagnostic stream.
    DiagnosticStream,
    /// Telemetry datagram with a negotiated maximum payload size in bytes.
    TelemetryDatagram { mtu_bytes: usize },
}

impl BackpressureSignal {
    /// Minimum retry delay in milliseconds.
    pub const MIN_RETRY_AFTER_MILLIS: u64 = 1;
    /// Maximum retry delay in milliseconds.
    pub const MAX_RETRY_AFTER_MILLIS: u64 = 60_000;
    /// Conservative upper bound on the encoded wire size of a backpressure
    /// signal. The struct is fully fixed-size on the wire (reason tag,
    /// optional request id, optional retry delay), so a small constant is
    /// sufficient and lets transport callers reject oversize datagrams
    /// without round-tripping through an encoder.
    pub const ENCODED_SIZE_UPPER_BOUND_BYTES: usize = 24;
    /// Minimum telemetry datagram payload that the project will negotiate
    /// before allowing backpressure to be carried over a datagram. This is
    /// well below the IETF QUIC `max_datagram_frame_size` floor used by
    /// Andromeda (1200 bytes), but we expose it as a named constant so the
    /// routing invariant is testable without binding to a concrete I/O
    /// backend.
    pub const MIN_DATAGRAM_MTU_BYTES: usize = Self::ENCODED_SIZE_UPPER_BOUND_BYTES;

    /// Validates retry policy constraints.
    pub fn validate_retry_policy(&self) -> AndromedaResult<()> {
        let Some(retry_after_millis) = self.retry_after_millis else {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "backpressure signals must include a retry delay",
            ));
        };

        if !(Self::MIN_RETRY_AFTER_MILLIS..=Self::MAX_RETRY_AFTER_MILLIS)
            .contains(&retry_after_millis)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "backpressure retry delay is outside the accepted range",
            ));
        }

        if self.reason.is_request_scoped() && self.request_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "request-scoped backpressure requires a RequestId",
            ));
        }

        Ok(())
    }

    /// Returns true if backpressure is sendable on the given stream role.
    ///
    /// Backpressure is a control-plane concept and must never be multiplexed
    /// onto session-control, RPC command, or RPC result streams.
    pub const fn is_sendable_on(role: StreamRole) -> bool {
        matches!(role, StreamRole::Diagnostic | StreamRole::TelemetryDatagram)
    }

    /// Validates that this backpressure signal may be routed on the given
    /// stream role. Rejects with [`AndromedaErrorKind::Protocol`] when the
    /// role is not a permitted backpressure carrier.
    pub fn validate_routing(&self, role: StreamRole) -> AndromedaResult<()> {
        if Self::is_sendable_on(role) {
            return Ok(());
        }
        Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "backpressure signals are only sendable on diagnostic stream or telemetry datagram",
        ))
    }

    /// Validates the signal for a concrete transport surface.
    ///
    /// On a reliable diagnostic stream this only enforces retry-policy
    /// bounds. On a telemetry datagram it additionally rejects negotiated MTUs
    /// that cannot accommodate the encoded form, and rejects any signal whose
    /// reason is request-scoped without a RequestId (because a fire-and-forget
    /// datagram cannot fall back to a reliable retry).
    pub fn validate_for_transport(&self, transport: BackpressureTransport) -> AndromedaResult<()> {
        self.validate_retry_policy()?;

        match transport {
            BackpressureTransport::DiagnosticStream => Ok(()),
            BackpressureTransport::TelemetryDatagram { mtu_bytes } => {
                if mtu_bytes < Self::MIN_DATAGRAM_MTU_BYTES {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Resource,
                        "backpressure signal exceeds negotiated QUIC DATAGRAM MTU",
                    ));
                }
                if self.reason.is_request_scoped() && self.request_id.is_none() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Protocol,
                        "request-scoped backpressure cannot be carried by an unaddressed datagram",
                    ));
                }
                Ok(())
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backpressure_retry_policy_requires_bounded_retry_delay() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::WalFlushLag,
            request_id: None,
            retry_after_millis: Some(250),
        };

        assert!(signal.validate_retry_policy().is_ok());

        let zero_retry = BackpressureSignal {
            retry_after_millis: Some(0),
            ..signal
        };

        assert_eq!(
            zero_retry.validate_retry_policy().unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );

        let missing_retry = BackpressureSignal {
            retry_after_millis: None,
            ..signal
        };

        assert_eq!(
            missing_retry.validate_retry_policy().unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );
    }

    #[test]
    fn request_scoped_backpressure_requires_request_id() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::ExecutionQueueSaturated,
            request_id: None,
            retry_after_millis: Some(500),
        };

        assert_eq!(
            signal.validate_retry_policy().unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );

        let scoped = BackpressureSignal {
            request_id: Some(RequestId::new(42)),
            ..signal
        };

        assert!(scoped.validate_retry_policy().is_ok());
    }

    #[test]
    fn backpressure_routing_only_on_diagnostic_or_telemetry() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::WalFlushLag,
            request_id: None,
            retry_after_millis: Some(50),
        };

        assert!(signal.validate_routing(StreamRole::Diagnostic).is_ok());
        assert!(
            signal
                .validate_routing(StreamRole::TelemetryDatagram)
                .is_ok()
        );

        for bad in [
            StreamRole::SessionControl,
            StreamRole::CommandBidirectional,
            StreamRole::ResultUnidirectional,
        ] {
            assert_eq!(
                signal.validate_routing(bad).unwrap_err().kind(),
                AndromedaErrorKind::Protocol,
                "role {bad:?} must reject backpressure"
            );
        }
    }

    #[test]
    fn backpressure_datagram_rejects_undersized_mtu() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::WalFlushLag,
            request_id: None,
            retry_after_millis: Some(50),
        };

        let err = signal
            .validate_for_transport(BackpressureTransport::TelemetryDatagram {
                mtu_bytes: BackpressureSignal::ENCODED_SIZE_UPPER_BOUND_BYTES - 1,
            })
            .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Resource);

        assert!(
            signal
                .validate_for_transport(BackpressureTransport::TelemetryDatagram {
                    mtu_bytes: 1200
                })
                .is_ok()
        );
    }

    #[test]
    fn backpressure_datagram_rejects_unaddressed_request_scoped_signals() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::ExecutionQueueSaturated,
            request_id: None,
            retry_after_millis: Some(50),
        };

        // The retry policy itself already rejects unaddressed request-scoped
        // signals; ensure the datagram path also surfaces a typed error and
        // that providing a RequestId fixes the routing.
        assert!(
            signal
                .validate_for_transport(BackpressureTransport::TelemetryDatagram {
                    mtu_bytes: 1200
                })
                .is_err()
        );

        let addressed = BackpressureSignal {
            request_id: Some(RequestId::new(7)),
            ..signal
        };
        assert!(
            addressed
                .validate_for_transport(BackpressureTransport::TelemetryDatagram {
                    mtu_bytes: 1200
                })
                .is_ok()
        );
    }

    #[test]
    fn backpressure_diagnostic_stream_only_enforces_retry_policy() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::WalFlushLag,
            request_id: None,
            retry_after_millis: Some(250),
        };
        assert!(
            signal
                .validate_for_transport(BackpressureTransport::DiagnosticStream)
                .is_ok()
        );

        let bad = BackpressureSignal {
            retry_after_millis: Some(BackpressureSignal::MAX_RETRY_AFTER_MILLIS + 1),
            ..signal
        };
        assert_eq!(
            bad.validate_for_transport(BackpressureTransport::DiagnosticStream)
                .unwrap_err()
                .kind(),
            AndromedaErrorKind::Resource
        );
    }
}
