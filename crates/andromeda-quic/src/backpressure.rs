use andromeda_core::RequestId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureReason {
    ReceiveBufferSaturated,
    SlowClient,
    ExecutionQueueSaturated,
    WalFlushLag,
    HotStorePressure,
    TempStoreQuota,
    ResultSpoolGrowth,
    CatalogLockContention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureSignal {
    pub reason: BackpressureReason,
    pub request_id: Option<RequestId>,
    pub retry_after_millis: Option<u64>,
}
