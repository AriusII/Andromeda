//! Backpressured result stream with bounded queue and durable completion evidence.

mod backpressure;
mod frames;

use crate::validation::completion_status_transaction_state;
use crate::{CompletionStatus, ResultStreamMetadata};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_rpc_protocol::{BackpressureReason, BackpressureSignal};
use andromeda_structured_object::StructuredObjectHeader;
use andromeda_types::RequestId;
use andromeda_wal::Lsn;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, RwLock, mpsc};

use backpressure::ResultStreamMetricsInner;
use frames::{ResultStreamFrame, ResultStreamMessage};

pub const DEFAULT_RESULT_STREAM_CAPACITY: usize = 1024;
pub const MAX_RESULT_STREAM_CAPACITY: usize = 1_000_000;
pub const MIN_RESULT_STREAM_CAPACITY: usize = 1;

pub use backpressure::ResultStreamMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamCompletion {
    status: CompletionStatus,
    lsn: u64,
    row_count: u64,
}

impl StreamCompletion {
    pub fn new(status: CompletionStatus, lsn: u64, row_count: u64) -> AndromedaResult<Self> {
        if !status.is_transactional_terminal() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "result stream completion requires terminal status; got {:?}",
                    status
                ),
            ));
        }

        if lsn == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "result stream completion requires nonzero durable LSN evidence",
            ));
        }

        if status == CompletionStatus::RolledBack && row_count != 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "rolled-back result stream completion must report zero rows",
            ));
        }

        Ok(Self {
            status,
            lsn,
            row_count,
        })
    }

    pub const fn status(&self) -> CompletionStatus {
        self.status
    }

    pub const fn lsn(&self) -> u64 {
        self.lsn
    }

    pub const fn row_count(&self) -> u64 {
        self.row_count
    }
}

pub struct BackpressuredResultStream {
    metadata: Option<ResultStreamMetadata>,
    tx: mpsc::Sender<ResultStreamMessage>,
    rx: Arc<Mutex<mpsc::Receiver<ResultStreamMessage>>>,
    completion: Arc<Mutex<Option<StreamCompletion>>>,
    completed: Arc<AtomicBool>,
    emission_gate: Arc<RwLock<()>>,
    metrics: Arc<ResultStreamMetricsInner>,
    capacity: usize,
}

impl BackpressuredResultStream {
    pub fn new(capacity: usize) -> AndromedaResult<Self> {
        if !(MIN_RESULT_STREAM_CAPACITY..=MAX_RESULT_STREAM_CAPACITY).contains(&capacity) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "result stream capacity {} is outside range [{}, {}]",
                    capacity, MIN_RESULT_STREAM_CAPACITY, MAX_RESULT_STREAM_CAPACITY
                ),
            ));
        }

        let (tx, rx) = mpsc::channel(capacity);

        Ok(Self {
            metadata: None,
            tx,
            rx: Arc::new(Mutex::new(rx)),
            completion: Arc::new(Mutex::new(None)),
            completed: Arc::new(AtomicBool::new(false)),
            emission_gate: Arc::new(RwLock::new(())),
            metrics: Arc::new(ResultStreamMetricsInner::new()),
            capacity,
        })
    }

    pub fn emit_metadata(&mut self, metadata: ResultStreamMetadata) -> AndromedaResult<()> {
        if self.metadata.is_some() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream metadata already emitted",
            ));
        }

        metadata.validate_before_payload()?;

        self.metadata = Some(metadata);
        Ok(())
    }

    pub fn metadata(&self) -> Option<&ResultStreamMetadata> {
        self.metadata.as_ref()
    }

    pub async fn push_row(&self, row: StructuredObjectHeader) -> AndromedaResult<()> {
        let _admission = self.emission_gate.read().await;

        if self.completed.load(Ordering::Acquire) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream is already completed",
            ));
        }

        self.validate_row_before_enqueue(&row)?;

        let message = match self.tx.try_send(ResultStreamMessage::row(row)) {
            Ok(_) => {
                self.metrics.record_row_pushed();
                return Ok(());
            }
            Err(mpsc::error::TrySendError::Full(message)) => {
                self.metrics.record_backpressure();
                message
            }
            Err(mpsc::error::TrySendError::Closed(_message)) => {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    "result stream channel closed",
                ));
            }
        };

        self.tx.send(message).await.map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                "result stream channel closed during backpressure",
            )
        })?;

        self.metrics.record_row_pushed();
        Ok(())
    }

    pub async fn complete(&self, status: CompletionStatus, lsn: u64) -> AndromedaResult<()> {
        let _terminal = self.emission_gate.write().await;

        if !status.is_transactional_terminal() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!(
                    "result stream completion requires terminal status; got {:?}",
                    status
                ),
            ));
        }

        if lsn == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "result stream completion requires nonzero durable LSN evidence",
            ));
        }
        let transaction_state = completion_status_transaction_state(status)?;

        let metadata = self.metadata.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream metadata must be emitted before terminal completion",
            )
        })?;

        if self.completed.swap(true, Ordering::AcqRel) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream completion already emitted",
            ));
        }

        let row_count = self.metrics.total_rows_pushed();
        if let Err(error) =
            metadata.validate_terminal_completion(transaction_state, Lsn::new(lsn), row_count)
        {
            self.completed.store(false, Ordering::Release);
            return Err(error);
        }

        let completion = StreamCompletion::new(status, lsn, row_count)?;

        {
            let mut guard = self.completion.lock().await;
            *guard = Some(completion);
        }

        if let Err(error) = self
            .tx
            .send(ResultStreamMessage::completion(completion))
            .await
        {
            self.completed.store(false, Ordering::Release);
            let mut guard = self.completion.lock().await;
            *guard = None;
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("result stream channel closed during completion: {error}"),
            ));
        }

        Ok(())
    }

    pub async fn completion(&self) -> Option<StreamCompletion> {
        let guard = self.completion.lock().await;
        *guard
    }

    async fn next_frame(&self) -> Option<ResultStreamFrame> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(ResultStreamMessage::Row(row)) => {
                self.metrics.record_row_consumed();
                Some(ResultStreamFrame::Row(row))
            }
            Some(ResultStreamMessage::Completion(completion)) => {
                Some(ResultStreamFrame::Completion(completion))
            }
            None => None,
        }
    }

    pub async fn next_row(&self) -> Option<StructuredObjectHeader> {
        match self.next_frame().await {
            Some(ResultStreamFrame::Row(row)) => Some(row),
            Some(ResultStreamFrame::Completion(_)) | None => None,
        }
    }

    pub fn metrics(&self) -> ResultStreamMetrics {
        self.metrics.snapshot()
    }

    pub fn is_backpressured(&self) -> bool {
        let depth = self.metrics.queue_depth();
        depth > (self.capacity * 3 / 4)
    }

    pub fn backpressure_signal(&self, request_id: Option<RequestId>) -> Option<BackpressureSignal> {
        let request_id = request_id?;

        if !self.is_backpressured() {
            return None;
        }

        Some(BackpressureSignal {
            reason: BackpressureReason::ResultSpoolGrowth,
            request_id: Some(request_id),
            retry_after_millis: Some(10),
        })
    }

    fn validate_row_before_enqueue(&self, row: &StructuredObjectHeader) -> AndromedaResult<()> {
        let metadata = self.metadata.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream metadata must be emitted before payload rows",
            )
        })?;

        row.validate()?;

        if row.column_count != metadata.column_count {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "result stream row column_count {} does not match metadata column_count {}",
                    row.column_count, metadata.column_count
                ),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_creation() {
        let result = BackpressuredResultStream::new(10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_queue_capacity_validation() {
        let result = BackpressuredResultStream::new(0);
        assert!(result.is_err());

        let result = BackpressuredResultStream::new(MAX_RESULT_STREAM_CAPACITY + 1);
        assert!(result.is_err());

        let result = BackpressuredResultStream::new(100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_completion_type_construction_is_validated() {
        let completion = StreamCompletion::new(CompletionStatus::Committed, 42, 10).unwrap();
        assert_eq!(completion.status(), CompletionStatus::Committed);
        assert_eq!(completion.lsn(), 42);
        assert_eq!(completion.row_count(), 10);

        assert!(StreamCompletion::new(CompletionStatus::Committed, 0, 10).is_err());
        assert!(StreamCompletion::new(CompletionStatus::RolledBack, 42, 1).is_err());
    }
}
