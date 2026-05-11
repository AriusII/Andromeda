//! ForensicStartupGate — C5 Application Surface block for [`ForensicStart`] mode.
//!
//! This module implements the C5 fence described in P13 exit criterion line 74:
//! *"ForensicStart produit rapport et bloque trafic applicatif."*
//!
//! The gate exists **at type level** as of P13/W3.  The actual integration into
//! the QUIC bootstrap listener is a **P14+ integration task** — see the TODO
//! comment on [`block_application_surface`].
//!
//! [`ForensicStart`]: crate::StartupMode::ForensicStart

use crate::StartupMode;

/// Disposition of the Application Surface for a given startup mode.
///
/// Callers in the QUIC bootstrap path, RPC admission, and network ingress MUST
/// consult this type before opening any Application-plane connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationSurfaceDisposition {
    /// Application-plane traffic is permitted for this startup mode.
    Allow,
    /// Application-plane traffic is **blocked** — forensic investigation mode is active.
    ///
    /// Callers MUST refuse new Application-plane connections and return a
    /// service-unavailable response indicating that the node is in forensic mode.
    BlockForensic,
}

/// Returns `true` if Application Surface traffic **MUST** be blocked for `mode`.
///
/// This is the C5 fence satisfying P13 exit criterion line 74.  Returns `true`
/// only for [`StartupMode::ForensicStart`]; all other startup modes return `false`.
///
/// # Integration TODO (P14+)
///
/// This function MUST be consulted by:
///
/// - **`andromeda-quic`**: before opening any Application-plane QUIC stream.
///   Integration site: `bootstrap::listen`, at the `Hello → Auth` state
///   transition where the Application-plane QUIC stream is first accepted.
/// - **`andromeda-rpc`**: before accepting any RPC session on the Application
///   plane.
/// - **`andromeda-admission`**: before issuing any admission token for the
///   Application surface scope.
///
/// The gate function exists at type level as of P13/W3.  Wiring it into the
/// QUIC listener and the RPC/admission paths is deferred to a P14+ integration
/// task.
pub fn block_application_surface(mode: StartupMode) -> bool {
    matches!(mode, StartupMode::ForensicStart)
}

/// Returns the [`ApplicationSurfaceDisposition`] for `mode`.
///
/// This is the richer form of [`block_application_surface`] intended for
/// callers that need a typed enum rather than a raw `bool` (e.g., admission
/// policy engines, audit traces, and decision-trace projections).
///
/// See [`block_application_surface`] for the full integration contract and
/// P14+ TODO.
pub fn application_surface_disposition(mode: StartupMode) -> ApplicationSurfaceDisposition {
    if block_application_surface(mode) {
        ApplicationSurfaceDisposition::BlockForensic
    } else {
        ApplicationSurfaceDisposition::Allow
    }
}
