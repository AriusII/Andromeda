use andromeda_wal::Lsn;

use crate::StartupMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTrace<TraceId = u128> {
    pub trace_id: TraceId,
    pub startup_mode: StartupMode,
    pub replay_start_lsn: Lsn,
}
