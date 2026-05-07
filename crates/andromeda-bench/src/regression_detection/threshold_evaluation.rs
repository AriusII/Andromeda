use super::reason::RegressionReason;

pub(super) struct ThresholdEvaluation {
    pub primary_reason: RegressionReason,
    pub regressed_count: u8,
    pub severity: u8,
}

pub(super) fn evaluate_thresholds(
    p50_regression_pct: f64,
    p95_regression_pct: f64,
    error_rate_regression_pct: f64,
) -> ThresholdEvaluation {
    const REGRESSION_THRESHOLD_PCT: f64 = 2.5;
    let p50_regressed = p50_regression_pct > REGRESSION_THRESHOLD_PCT;
    let p95_regressed = p95_regression_pct > REGRESSION_THRESHOLD_PCT;
    let error_regressed = error_rate_regression_pct > 0.0;

    let (primary_reason, regressed_count) = if p50_regressed || p95_regressed || error_regressed {
        let mut count = 0;
        if p50_regressed {
            count += 1;
        }
        if p95_regressed {
            count += 1;
        }
        if error_regressed {
            count += 1;
        }

        let reason = if count > 1 {
            RegressionReason::MultipleMetrics
        } else if p50_regressed {
            RegressionReason::P50Degradation
        } else if p95_regressed {
            RegressionReason::P95Degradation
        } else {
            RegressionReason::ErrorRateIncrease
        };
        (reason, count)
    } else {
        (RegressionReason::NoRegression, 0)
    };

    let max_regression = p50_regression_pct
        .max(p95_regression_pct)
        .max(error_rate_regression_pct);
    let severity = if regressed_count == 0 {
        0
    } else if max_regression < 5.0 {
        1
    } else if max_regression < 20.0 {
        2
    } else {
        3
    };

    ThresholdEvaluation {
        primary_reason,
        regressed_count,
        severity,
    }
}
