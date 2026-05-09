//! Comprehensive contract tests for SRPL result metadata extraction.
//!
//! This contract test suite verifies that the result metadata extraction logic
//! correctly handles declared Procedure body shapes and
//! produces deterministic, valid metadata ready for emission before payload.
//!
//! Test Coverage:
//! - Extract metadata from EMIT operations with single/multiple values
//! - Extract metadata from READ operations with different cardinalities
//! - Extract metadata from mutation operations with exact/unknown row counts
//! - Validate cardinality inference from operation types
//! - Validate stream_id and column_count pass-through
//! - Validate row_count_exact and row_count_max are set correctly
//! - Validate deterministic extraction (same input -> same output)
//! - Validate error handling for invalid inputs

#[path = "metadata_extraction_contract/emit_shapes.rs"]
mod emit_shapes;
#[path = "metadata_extraction_contract/metadata_contract.rs"]
mod metadata_contract;
#[path = "metadata_extraction_contract/mutation_counts.rs"]
mod mutation_counts;
#[path = "metadata_extraction_contract/read_cardinality.rs"]
mod read_cardinality;
#[path = "metadata_extraction_contract/support.rs"]
mod support;
