//! SRPL Execution Validation Gates — H1-SRPL-EXEC-007
//!
//! Comprehensive validation gates verifying:
//! - All lexer token types are exercised
//! - All parser grammar rules are exercised
//! - Type binding covers all cases
//! - IR lowering exercises all node types
//! - Dispatcher paths (V0, SRPL) are exercised
//! - No panics in error paths
//! - Performance baselines are met
//! - Thread safety is verified
//!
//! Exit status: All gates must pass for production readiness.

#[path = "validation_gates/contract_validation.rs"]
mod contract_validation;
#[path = "validation_gates/error_paths.rs"]
mod error_paths;
#[path = "validation_gates/ir_lowering.rs"]
mod ir_lowering;
#[path = "validation_gates/lexer_parser.rs"]
mod lexer_parser;
#[path = "validation_gates/performance_concurrency.rs"]
mod performance_concurrency;
#[path = "validation_gates/summary.rs"]
mod summary;
#[path = "validation_gates/support.rs"]
mod support;
#[path = "validation_gates/type_binding.rs"]
mod type_binding;
