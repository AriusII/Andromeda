use andromeda_core::{RequestId, SessionId};

/// Origin of a cancellation signal.
///
/// The cause is part of the typed protocol surface: it lets dispatchers
/// distinguish a client-initiated abort from a server-side timeout or
/// administrative kill, and lets the lifecycle gate reject server-only
/// causes when they arrive in the wrong state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancellationCause {
    /// Client explicitly cancelled the in-flight request.
    ClientRequested,
    /// Per-request timeout fired.
    Timeout,
    /// Administrative abort issued out-of-band (e.g. operator drain).
    AdminAbort,
    /// Server is draining the session; in-flight commands are being
    /// terminated as part of the drain protocol.
    SessionDraining,
    /// Server has closed the session; emitted as a final terminator.
    SessionClosed,
}

/// Typed cancellation control message.
///
/// Cancellation is request-scoped and session-scoped: it must always carry
/// a [`RequestId`] (cancellation of "everything on the session" is modeled
/// at the lifecycle layer via `Connection::begin_drain` and
/// `Connection::close`, not here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancellationSignal {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub cause: CancellationCause,
}

/// Outcome of routing a cancellation signal through the lifecycle gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationOutcome {
    /// The session is `Active` and the signal targets an in-flight request.
    Delivered,
    /// The session is `Draining`; the signal is permitted because in-flight
    /// requests are still allowed to complete or be aborted during drain.
    DeliveredDuringDrain,
}
