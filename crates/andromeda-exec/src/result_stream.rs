//! Backpressured result stream with bounded queue and durable completion evidence.

use crate::{CompletionStatus, ResultStreamMetadata};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId};
use andromeda_proto::StructuredObjectHeader;
use andromeda_quic::{BackpressureReason, BackpressureSignal};
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tokio::sync::mpsc;

pub const DEFAULT_RESULT_STREAM_CAPACITY: usize = 1024;
pub const MAX_RESULT_STREAM_CAPACITY: usize = 1_000_000;
pub const MIN_RESULT_STREAM_CAPACITY: usize = 1;

#[derive(Debug)]
enum ResultStreamMessage {
    Row(StructuredObjectHeader),
    Completion,
}

#[derive(Debug, Clone)]
pub struct ResultStreamMetrics {
    pub queue_depth: usize,
    pub total_rows_pushed: u64,
    pub total_rows_consumed: u64,
    pub backpressure_count: u64,
    pub peak_queue_depth: usize,
    pub memory_usage_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamCompletion {
    pub status: CompletionStatus,
    pub lsn: u64,
    pub row_count: u64,
}

pub struct BackpressuredResultStream {
    metadata: Option<ResultStreamMetadata>,
    tx: mpsc::Sender<ResultStreamMessage>,
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<ResultStreamMessage>>>,
    completion: Arc<tokio::sync::Mutex<Option<StreamCompletion>>>,
    completed: Arc<AtomicBool>,
    metrics: Arc<ResultStreamMetricsInner>,
    capacity: usize,
}

struct ResultStreamMetricsInner {
    queue_depth: AtomicUsize,
    total_rows_pushed: AtomicU64,
    total_rows_consumed: AtomicU64,
    backpressure_count: AtomicU64,
    peak_queue_depth: AtomicUsize,
    memory_usage_bytes: AtomicU64,
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
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
            completion: Arc::new(tokio::sync::Mutex::new(None)),
            completed: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(ResultStreamMetricsInner {
                queue_depth: AtomicUsize::new(0),
                total_rows_pushed: AtomicU64::new(0),
                total_rows_consumed: AtomicU64::new(0),
                backpressure_count: AtomicU64::new(0),
                peak_queue_depth: AtomicUsize::new(0),
                memory_usage_bytes: AtomicU64::new(0),
            }),
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
        if self.completed.load(Ordering::Acquire) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream is already completed",
            ));
        }

        if self.metadata.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "result stream metadata must be emitted before payload rows",
            ));
        }

        let message = match self.tx.try_send(ResultStreamMessage::Row(row)) {
            Ok(_) => {
                self.metrics
                    .total_rows_pushed
                    .fetch_add(1, Ordering::Relaxed);
                self.update_queue_depth_metric();
                return Ok(());
            }
            Err(mpsc::error::TrySendError::Full(message)) => {
                self.metrics
                    .backpressure_count
                    .fetch_add(1, Ordering::Relaxed);
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

        self.metrics
            .total_rows_pushed
            .fetch_add(1, Ordering::Relaxed);
        self.update_queue_depth_metric();
        Ok(())
    }

    pub async fn complete(&self, status: CompletionStatus, lsn: u64) -> AndromedaResult<()> {
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

        let row_count = self.metrics.total_rows_pushed.load(Ordering::Acquire);
        if let Err(error) =
            metadata.validate_terminal_completion(transaction_state, Lsn::new(lsn), row_count)
        {
            self.completed.store(false, Ordering::Release);
            return Err(error);
        }

        let completion = StreamCompletion {
            status,
            lsn,
            row_count,
        };

        {
            let mut guard = self.completion.lock().await;
            *guard = Some(completion);
        }

        if let Err(error) = self.tx.send(ResultStreamMessage::Completion).await {
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

    pub async fn next_row(&self) -> Option<StructuredObjectHeader> {
        let mut rx = self.rx.lock().await;
        match rx.recv().await {
            Some(ResultStreamMessage::Row(row)) => {
                self.metrics
                    .total_rows_consumed
                    .fetch_add(1, Ordering::Relaxed);
                self.update_queue_depth_metric();
                Some(row)
            }
            Some(ResultStreamMessage::Completion) | None => None,
        }
    }

    pub fn metrics(&self) -> ResultStreamMetrics {
        ResultStreamMetrics {
            queue_depth: self.metrics.queue_depth.load(Ordering::Relaxed),
            total_rows_pushed: self.metrics.total_rows_pushed.load(Ordering::Relaxed),
            total_rows_consumed: self.metrics.total_rows_consumed.load(Ordering::Relaxed),
            backpressure_count: self.metrics.backpressure_count.load(Ordering::Relaxed),
            peak_queue_depth: self.metrics.peak_queue_depth.load(Ordering::Relaxed),
            memory_usage_bytes: self.metrics.memory_usage_bytes.load(Ordering::Relaxed),
        }
    }

    pub fn is_backpressured(&self) -> bool {
        let depth = self.metrics.queue_depth.load(Ordering::Relaxed);
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

    fn update_queue_depth_metric(&self) {
        let pushed = self.metrics.total_rows_pushed.load(Ordering::Relaxed) as usize;
        let consumed = self.metrics.total_rows_consumed.load(Ordering::Relaxed) as usize;
        let depth = pushed.saturating_sub(consumed);

        self.metrics.queue_depth.store(depth, Ordering::Relaxed);

        let mut peak = self.metrics.peak_queue_depth.load(Ordering::Relaxed);
        while depth > peak {
            match self.metrics.peak_queue_depth.compare_exchange(
                peak,
                depth,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }
}

fn completion_status_transaction_state(
    status: CompletionStatus,
) -> AndromedaResult<TransactionState> {
    match status {
        CompletionStatus::Committed => Ok(TransactionState::Committed),
        CompletionStatus::RolledBack => Ok(TransactionState::RolledBack),
        _ => Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            format!(
                "result stream completion requires terminal status; got {:?}",
                status
            ),
        )),
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
    fn test_completion_type_construction() {
        let completion = StreamCompletion {
            status: CompletionStatus::Committed,
            lsn: 42,
            row_count: 10,
        };
        assert_eq!(completion.status, CompletionStatus::Committed);
        assert_eq!(completion.lsn, 42);
        assert_eq!(completion.row_count, 10);
    }
}
