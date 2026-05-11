//! Prototype GPU binding contexts for statistics and analytics workloads.
//!
//! This module contains two prototype binding contexts:
//!
//! - [`stats::GpuStatsBindingContext`] — `GPU_STATS`: histogram computation with
//!   CPU shadow validation.
//! - [`analytics::GpuAnalyticsBindingContext`] — `GPU_ANALYTICS`: columnar
//!   analytics scan with equality-based CPU shadow validation.
//! - [`permit`] — prevalidated execution permits that enforce policy approval
//!   before prototype execution.
//!
//! Both contexts enforce the same execution contract:
//! 1. Kill switch check — cancels immediately if active.
//! 2. GPU availability check — falls back to CPU if GPU is absent.
//! 3. GPU closure invocation.
//! 4. CPU shadow execution.
//! 5. Validation gate — uses CPU result if GPU output is rejected.

pub mod analytics;
pub mod permit;
pub mod stats;
