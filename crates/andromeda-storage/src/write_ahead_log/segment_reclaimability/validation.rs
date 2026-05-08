use crate::Lsn;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

fn storage_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

pub(super) fn validate_retention_boundary_order(
    required_recovery_lsn: Lsn,
    min_active_snapshot_lsn: Lsn,
) -> AndromedaResult<()> {
    if required_recovery_lsn > min_active_snapshot_lsn {
        return Err(storage_error(
            "required_recovery_lsn must not exceed min_active_snapshot_lsn",
        ));
    }

    Ok(())
}
