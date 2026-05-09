use andromeda_rpc_protocol::FrameFamily;

/// Surface plane bound to a QUIC session at handshake time.
///
/// The plane fixes the operational tenancy of every frame admitted on the
/// connection. It is a purely typed value; concrete plane-to-port routing is
/// the responsibility of the listener layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfacePlane {
    /// Tenant-facing application traffic (read/write, contracts, RPC).
    Application,
    /// Operator administration: catalog, schema, governance, drain commands.
    Administration,
    /// HADR / cluster control plane (replication, failover, cluster membership).
    HighAvailability,
    /// Read-only monitoring and diagnostics surface.
    Monitoring,
}

impl SurfacePlane {
    /// Returns true when this plane is the tenant-facing Application surface.
    pub const fn is_application(self) -> bool {
        matches!(self, Self::Application)
    }

    /// Returns true if this plane permits a frame family at the transport level.
    ///
    /// Session control and diagnostic frames are allowed on every plane so
    /// that handshake and error reporting always remain reachable.
    pub const fn permits_family(self, family: FrameFamily) -> bool {
        matches!(
            (self, family),
            (_, FrameFamily::SessionControl)
                | (_, FrameFamily::Diagnostic)
                | (Self::Application, FrameFamily::ContractControl)
                | (Self::Application, FrameFamily::RpcCommand)
                | (Self::Application, FrameFamily::RpcResultStream)
                | (Self::Application, FrameFamily::Telemetry)
                | (Self::Administration, FrameFamily::ContractControl)
                | (Self::Administration, FrameFamily::RpcCommand)
                | (Self::Administration, FrameFamily::RpcResultStream)
                | (Self::Administration, FrameFamily::Telemetry)
                | (Self::HighAvailability, FrameFamily::ContractControl)
                | (Self::HighAvailability, FrameFamily::RpcCommand)
                | (Self::HighAvailability, FrameFamily::RpcResultStream)
                | (Self::HighAvailability, FrameFamily::Telemetry)
                | (Self::Monitoring, FrameFamily::Telemetry)
        )
    }
}
