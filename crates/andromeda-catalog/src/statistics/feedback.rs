#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanFeedback {
    pub estimated_rows: u64,
    pub actual_rows: u64,
}

impl PlanFeedback {
    pub fn new(estimated_rows: u64, actual_rows: u64) -> (Self, f64) {
        (
            Self {
                estimated_rows,
                actual_rows,
            },
            row_error_ratio(estimated_rows, actual_rows),
        )
    }

    pub fn is_high_error(&self, threshold: f64) -> bool {
        row_error_ratio(self.estimated_rows, self.actual_rows) > threshold
    }
}

fn row_error_ratio(estimated_rows: u64, actual_rows: u64) -> f64 {
    let max_rows = estimated_rows.max(actual_rows);
    if max_rows == 0 {
        return 0.0;
    }
    estimated_rows.abs_diff(actual_rows) as f64 / max_rows as f64
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackStatistics {
    pub total_observations: u64,
    pub sum_errors: f64,
    pub max_error: f64,
    pub high_error_threshold: f64,
}

impl FeedbackStatistics {
    pub fn new(high_error_threshold: f64) -> Self {
        Self {
            total_observations: 0,
            sum_errors: 0.0,
            max_error: 0.0,
            high_error_threshold,
        }
    }

    pub fn record(&mut self, error_ratio: f64) {
        self.total_observations += 1;
        self.sum_errors += error_ratio;
        self.max_error = self.max_error.max(error_ratio);
    }

    pub fn average_error(&self) -> f64 {
        if self.total_observations > 0 {
            self.sum_errors / self.total_observations as f64
        } else {
            0.0
        }
    }

    pub fn should_recollect(&self) -> bool {
        self.max_error > self.high_error_threshold
            || self.average_error() > self.high_error_threshold
    }
}
