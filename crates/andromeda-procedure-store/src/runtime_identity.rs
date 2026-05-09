use andromeda_plan_cache::PlanCacheKey;

use crate::runtime_record::{InvocationRuntimeRecord, compute_runtime_record_id};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcedureRuntimeRecordId([u8; Self::LEN]);

impl ProcedureRuntimeRecordId {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn from_record(record: &InvocationRuntimeRecord) -> Self {
        Self(compute_runtime_record_id(record))
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

/// Stable identifier for the plan used by an invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcedureRuntimePlanId([u8; Self::LEN]);

impl ProcedureRuntimePlanId {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn from_plan_cache_key(key: &PlanCacheKey) -> Self {
        Self((*key).digest())
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}
