use super::{state_machine::Connection, surface::SurfacePlane};

/// QUIC/TLS early-data admission policy for a surface listener.
///
/// V0 intentionally exposes only `Disabled`: until a later decision scopes
/// replay-safe behavior, listener runtime wiring must not admit 0-RTT traffic
/// for any plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EarlyDataPolicy {
    /// Reject QUIC/TLS 0-RTT early data.
    Disabled,
}

/// QUIC datagram admission policy for a surface listener.
///
/// The V0 transport model is stream/frame based. Datagrams are not available
/// as a shortcut for WAL shipping, HA/DR state transfer, or generic telemetry
/// bypass paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatagramPolicy {
    /// QUIC datagrams are disabled for this listener.
    Disabled,
}

/// Runtime-free listener/session contract for one surface plane.
///
/// The value deliberately contains no socket address, certificate, executor, or
/// stream-manager state. A listener owns exactly one [`SurfacePlane`], always
/// disables early data, and opens sessions bound to the listener plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceListenerConfig {
    plane: SurfacePlane,
    early_data: EarlyDataPolicy,
    datagrams: DatagramPolicy,
}

impl SurfaceListenerConfig {
    /// Builds the listener/session contract for one surface plane.
    pub const fn for_plane(plane: SurfacePlane) -> Self {
        Self {
            plane,
            early_data: EarlyDataPolicy::Disabled,
            datagrams: DatagramPolicy::Disabled,
        }
    }

    /// Builds the Application-plane listener/session contract.
    pub const fn application() -> Self {
        Self::for_plane(SurfacePlane::Application)
    }

    /// Builds the Administration-plane listener/session contract.
    pub const fn administration() -> Self {
        Self::for_plane(SurfacePlane::Administration)
    }

    /// Builds the HA/DR-plane listener/session contract.
    pub const fn high_availability() -> Self {
        Self::for_plane(SurfacePlane::HighAvailability)
    }

    /// Builds the Monitoring-plane listener/session contract.
    pub const fn monitoring() -> Self {
        Self::for_plane(SurfacePlane::Monitoring)
    }

    /// Returns the single plane accepted by this listener.
    pub const fn surface_plane(&self) -> SurfacePlane {
        self.plane
    }

    /// Returns the listener early-data policy.
    pub const fn early_data_policy(&self) -> EarlyDataPolicy {
        self.early_data
    }

    /// Returns the listener datagram policy.
    pub const fn datagram_policy(&self) -> DatagramPolicy {
        self.datagrams
    }

    /// Creates a runtime-free session model bound to this listener's plane.
    pub const fn new_connection(&self) -> Connection {
        Connection::new(self.plane)
    }
}

/// Complete listener-per-plane configuration set.
///
/// Runtime wiring should create one concrete listener from each field rather
/// than multiplexing planes through a generic command surface. Certificate
/// identity, async executor handles, and stream runtime state are intentionally
/// excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceListenerSet {
    application: SurfaceListenerConfig,
    administration: SurfaceListenerConfig,
    high_availability: SurfaceListenerConfig,
    monitoring: SurfaceListenerConfig,
}

impl SurfaceListenerSet {
    /// Returns the canonical four-listener surface configuration.
    pub const fn standard() -> Self {
        Self {
            application: SurfaceListenerConfig::application(),
            administration: SurfaceListenerConfig::administration(),
            high_availability: SurfaceListenerConfig::high_availability(),
            monitoring: SurfaceListenerConfig::monitoring(),
        }
    }

    /// Application-plane listener/session configuration.
    pub const fn application(&self) -> SurfaceListenerConfig {
        self.application
    }

    /// Administration-plane listener/session configuration.
    pub const fn administration(&self) -> SurfaceListenerConfig {
        self.administration
    }

    /// HA/DR-plane listener/session configuration.
    pub const fn high_availability(&self) -> SurfaceListenerConfig {
        self.high_availability
    }

    /// Monitoring-plane listener/session configuration.
    pub const fn monitoring(&self) -> SurfaceListenerConfig {
        self.monitoring
    }
}
