use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::GcSchedulerExit;

/// Owned lifecycle handle for a spawned GC scheduler task.
///
/// Dropping this handle aborts the spawned task so callers cannot accidentally
/// detach an uncontrolled long-lived background task. Prefer [`Self::shutdown`]
/// for graceful cooperative shutdown evidence.
pub struct GcSchedulerHandle {
    pub(super) shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub(super) join: Option<tokio::task::JoinHandle<AndromedaResult<GcSchedulerExit>>>,
}

impl GcSchedulerHandle {
    /// Request cooperative shutdown. Returns `true` if this call delivered the
    /// first shutdown signal.
    pub fn request_shutdown(&mut self) -> bool {
        self.shutdown_tx
            .take()
            .map(|tx| tx.send(()).is_ok())
            .unwrap_or(false)
    }

    /// Request shutdown and await scheduler exit evidence.
    pub async fn shutdown(mut self) -> AndromedaResult<GcSchedulerExit> {
        let _ = self.request_shutdown();
        let join = self.join.take().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "gc scheduler join handle already consumed",
            )
        })?;

        join.await.map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("gc scheduler task join failed: {e}"),
            )
        })?
    }
}

impl Drop for GcSchedulerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.take().map(|tx| tx.send(()));
        if let Some(join) = &self.join {
            join.abort();
        }
    }
}
