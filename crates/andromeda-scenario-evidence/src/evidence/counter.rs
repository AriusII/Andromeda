use super::{MAX_BENCHMARK_WORKLOAD_COUNTER_NAME_BYTES, MAX_BENCHMARK_WORKLOAD_COUNTER_UNIT_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BenchmarkWorkloadCounter {
    pub name: &'static str,
    pub value: u64,
    pub unit: &'static str,
}

impl BenchmarkWorkloadCounter {
    pub const fn new(name: &'static str, value: u64, unit: &'static str) -> Self {
        Self { name, value, unit }
    }

    pub fn is_valid(self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= MAX_BENCHMARK_WORKLOAD_COUNTER_NAME_BYTES
            && !self.unit.trim().is_empty()
            && self.unit.len() <= MAX_BENCHMARK_WORKLOAD_COUNTER_UNIT_BYTES
    }
}
