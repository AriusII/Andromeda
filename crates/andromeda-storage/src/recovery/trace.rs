use andromeda_observe::TraceId;

use crate::Lsn;

use super::planning::StartupMode;

/// Storage-internal snapshot of the *inputs* that bound a recovery attempt:
/// the requested startup mode and the LSN from which redo is intended to begin.
///
/// This is intentionally distinct from [`andromeda_observe::RecoveryTrace`],
/// which is the canonical observability event emitted *after* recovery has
/// established a durable boundary (it carries `last_durable_lsn` and the
/// optional `corruption_boundary_lsn`). Authority over the wire/event format
/// lives in `andromeda-observe`; this struct exists only to let storage
/// callers tag forensic logs with the cold-snapshot recovery inputs without
/// taking a dependency on the full observe envelope.
///
/// Source-of-truth boundary: the cold snapshot (manifest) plus durable WAL
/// drive these fields. Hot/in-memory WAL state must never be used to populate
/// `replay_start_lsn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTrace {
    pub trace_id: TraceId,
    pub startup_mode: StartupMode,
    pub replay_start_lsn: Lsn,
}
