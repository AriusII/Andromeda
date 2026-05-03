use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId};

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

impl BackpressureReason {
    pub const fn is_request_scoped(self) -> bool {
        matches!(
            self,
            Self::SlowClient | Self::ExecutionQueueSaturated | Self::ResultSpoolGrowth
        )
    }
}

impl BackpressureSignal {
    pub const MIN_RETRY_AFTER_MILLIS: u64 = 1;
    pub const MAX_RETRY_AFTER_MILLIS: u64 = 60_000;

    pub fn validate_retry_policy(&self) -> AndromedaResult<()> {
        let Some(retry_after_millis) = self.retry_after_millis else {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "backpressure signals must include a retry delay",
            ));
        };

        if !(Self::MIN_RETRY_AFTER_MILLIS..=Self::MAX_RETRY_AFTER_MILLIS)
            .contains(&retry_after_millis)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "backpressure retry delay is outside the accepted range",
            ));
        }

        if self.reason.is_request_scoped() && self.request_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "request-scoped backpressure requires a RequestId",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backpressure_retry_policy_requires_bounded_retry_delay() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::WalFlushLag,
            request_id: None,
            retry_after_millis: Some(250),
        };

        assert!(signal.validate_retry_policy().is_ok());

        let zero_retry = BackpressureSignal {
            retry_after_millis: Some(0),
            ..signal
        };

        assert_eq!(
            zero_retry.validate_retry_policy().unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );

        let missing_retry = BackpressureSignal {
            retry_after_millis: None,
            ..signal
        };

        assert_eq!(
            missing_retry.validate_retry_policy().unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );
    }

    #[test]
    fn request_scoped_backpressure_requires_request_id() {
        let signal = BackpressureSignal {
            reason: BackpressureReason::ExecutionQueueSaturated,
            request_id: None,
            retry_after_millis: Some(500),
        };

        assert_eq!(
            signal.validate_retry_policy().unwrap_err().kind(),
            AndromedaErrorKind::Resource
        );

        let scoped = BackpressureSignal {
            request_id: Some(RequestId::new(42)),
            ..signal
        };

        assert!(scoped.validate_retry_policy().is_ok());
    }
}
