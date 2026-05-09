use crate::{
    EvidenceDigest, InvocationIdentity, InvocationMetricKind, ProcedureStorePrimitiveError,
    ProcedureStorePrimitiveResult,
};

pub const MAX_REGRESSION_THRESHOLD_BPS: u16 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegressionThresholdBps(u16);

impl RegressionThresholdBps {
    pub fn new(value: u16) -> ProcedureStorePrimitiveResult<Self> {
        if value > MAX_REGRESSION_THRESHOLD_BPS {
            return Err(ProcedureStorePrimitiveError::RegressionThresholdOutOfRange);
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegressionSeverity {
    None,
    Advisory,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegressionSignal {
    identity: InvocationIdentity,
    metric: InvocationMetricKind,
    baseline_value: u64,
    observed_value: u64,
    threshold_bps: RegressionThresholdBps,
    evidence_digest: EvidenceDigest,
}

impl RegressionSignal {
    pub const fn new(
        identity: InvocationIdentity,
        metric: InvocationMetricKind,
        baseline_value: u64,
        observed_value: u64,
        threshold_bps: RegressionThresholdBps,
        evidence_digest: EvidenceDigest,
    ) -> Self {
        Self {
            identity,
            metric,
            baseline_value,
            observed_value,
            threshold_bps,
            evidence_digest,
        }
    }

    pub const fn identity(self) -> InvocationIdentity {
        self.identity
    }

    pub const fn metric(self) -> InvocationMetricKind {
        self.metric
    }

    pub const fn baseline_value(self) -> u64 {
        self.baseline_value
    }

    pub const fn observed_value(self) -> u64 {
        self.observed_value
    }

    pub const fn threshold_bps(self) -> RegressionThresholdBps {
        self.threshold_bps
    }

    pub const fn evidence_digest(self) -> EvidenceDigest {
        self.evidence_digest
    }

    pub fn regression_bps(self) -> u64 {
        if self.observed_value <= self.baseline_value {
            return 0;
        }
        if self.baseline_value == 0 {
            return u64::from(MAX_REGRESSION_THRESHOLD_BPS);
        }

        let delta = u128::from(self.observed_value - self.baseline_value);
        let baseline = u128::from(self.baseline_value);
        let bps = delta.saturating_mul(10_000) / baseline;
        bps.min(u128::from(u64::MAX)) as u64
    }

    pub fn is_regression(self) -> bool {
        self.regression_bps() > u64::from(self.threshold_bps.get())
    }

    pub fn severity(self) -> RegressionSeverity {
        let bps = self.regression_bps();

        if bps <= u64::from(self.threshold_bps.get()) {
            RegressionSeverity::None
        } else if bps < 1_000 {
            RegressionSeverity::Advisory
        } else if bps < 2_500 {
            RegressionSeverity::Warning
        } else {
            RegressionSeverity::Critical
        }
    }

    pub const fn is_authoritative(self) -> bool {
        false
    }

    pub const fn can_select_plan_alone(self) -> bool {
        false
    }

    pub const fn is_durable_truth(self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InvocationId, ProcedureId};

    #[test]
    fn threshold_rejects_out_of_range_basis_points() {
        assert_eq!(
            RegressionThresholdBps::new(10_001).unwrap_err(),
            ProcedureStorePrimitiveError::RegressionThresholdOutOfRange
        );
        assert_eq!(RegressionThresholdBps::new(500).unwrap().get(), 500);
    }

    #[test]
    fn regression_signal_uses_finite_integer_basis_points() {
        let identity =
            InvocationIdentity::new(InvocationId::new(1).unwrap(), ProcedureId::new(2).unwrap());
        let digest = EvidenceDigest::new([7; EvidenceDigest::LEN]).unwrap();
        let signal = RegressionSignal::new(
            identity,
            InvocationMetricKind::DurationMillis,
            100,
            108,
            RegressionThresholdBps::new(500).unwrap(),
            digest,
        );

        assert_eq!(signal.regression_bps(), 800);
        assert!(signal.is_regression());
        assert_eq!(signal.severity(), RegressionSeverity::Advisory);
        assert_eq!(signal.evidence_digest(), digest);
        assert!(!signal.is_authoritative());
        assert!(!signal.can_select_plan_alone());
        assert!(!signal.is_durable_truth());
    }

    #[test]
    fn regression_signal_handles_zero_baseline_without_infinity() {
        let identity =
            InvocationIdentity::new(InvocationId::new(1).unwrap(), ProcedureId::new(2).unwrap());
        let signal = RegressionSignal::new(
            identity,
            InvocationMetricKind::WalBytes,
            0,
            1,
            RegressionThresholdBps::new(500).unwrap(),
            EvidenceDigest::new([8; EvidenceDigest::LEN]).unwrap(),
        );

        assert_eq!(signal.regression_bps(), 10_000);
        assert_eq!(signal.severity(), RegressionSeverity::Critical);
    }
}
