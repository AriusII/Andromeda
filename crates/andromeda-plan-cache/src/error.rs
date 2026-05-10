/// Errors raised while constructing a complete plan-cache identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheKeyError {
    ProcedureIdZero,
    CatalogVersionZero,
    ContractHashZero,
    StatsVersionZero,
    PolicyVersionZero,
    SingletonRejectsShapeFingerprint,
    ShapedPlanClassRequiresFingerprint,
}

impl core::fmt::Display for PlanCacheKeyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ProcedureIdZero => f.write_str("plan-cache key requires a non-zero ProcedureId"),
            Self::CatalogVersionZero => {
                f.write_str("plan-cache key requires a non-zero CatalogVersion")
            },
            Self::ContractHashZero => {
                f.write_str("plan-cache key requires a non-zero ContractHash")
            },
            Self::StatsVersionZero => {
                f.write_str("plan-cache key requires a non-zero StatsVersion")
            },
            Self::PolicyVersionZero => {
                f.write_str("plan-cache key requires a non-zero PolicyVersion")
            },
            Self::SingletonRejectsShapeFingerprint => {
                f.write_str("PlanClass::Singleton must use the empty PlanShapeFingerprint")
            },
            Self::ShapedPlanClassRequiresFingerprint => {
                f.write_str("non-Singleton PlanClass requires a non-empty PlanShapeFingerprint")
            },
        }
    }
}

impl std::error::Error for PlanCacheKeyError {}

/// Errors raised by plan-cache policy and reuse evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCachePolicyError {
    CapacityZero,
    CapacityTooLarge,
    PolicyVersionZero,
    TraceIdZero,
    TraceBuildFailed,
}

impl core::fmt::Display for PlanCachePolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapacityZero => f.write_str("enabled plan-cache capacity must be non-zero"),
            Self::CapacityTooLarge => {
                f.write_str("plan-cache capacity exceeds the bounded maximum")
            },
            Self::PolicyVersionZero => f.write_str("plan-cache policy version must not be zero"),
            Self::TraceIdZero => f.write_str("plan-cache trace id must not be zero"),
            Self::TraceBuildFailed => f.write_str("plan-cache decision trace construction failed"),
        }
    }
}

impl std::error::Error for PlanCachePolicyError {}
