//! Smoke test: the `CpuFallback` trait can be implemented and invoked without
//! any GPU code path being involved.

use andromeda_error::AndromedaResult;
use andromeda_gpu::{fallback::CpuFallback, prototypes::stats::StatsInput, validation::Histogram};

struct DeterministicCpuFallback {
    #[allow(dead_code)]
    expected_column_id: u32,
}

impl CpuFallback for DeterministicCpuFallback {
    type Input = StatsInput;
    type Output = Histogram;

    fn execute(&self, input: &StatsInput) -> AndromedaResult<Histogram> {
        let count = u64::from(input.column_id) + 1;
        Ok(Histogram {
            buckets: vec![count, count * 2, count * 3],
        })
    }
}

#[test]
fn cpu_fallback_can_be_implemented_and_called_without_gpu() {
    let fallback = DeterministicCpuFallback {
        expected_column_id: 7,
    };
    let input = StatsInput {
        snapshot_lsn: 0,
        column_id: 7,
    };

    let result = fallback.execute(&input).unwrap();
    assert_eq!(result.buckets, vec![8, 16, 24]);
}

#[test]
fn cpu_fallback_is_send_and_sync() {
    // The trait bound requires Send + Sync; this verifies that a concrete
    // implementation satisfies these bounds without any GPU runtime.
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    let fallback = DeterministicCpuFallback {
        expected_column_id: 0,
    };
    assert_send_sync(&fallback);
}

#[test]
fn cpu_fallback_can_be_called_multiple_times_independently() {
    let fallback = DeterministicCpuFallback {
        expected_column_id: 0,
    };
    for column_id in 0..4_u32 {
        let input = StatsInput {
            snapshot_lsn: 0,
            column_id,
        };
        let result = fallback.execute(&input).unwrap();
        let expected = u64::from(column_id) + 1;
        assert_eq!(result.buckets[0], expected);
    }
}
