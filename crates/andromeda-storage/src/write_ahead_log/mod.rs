//! Write-ahead log domain facade.
//!
//! The WAL record model, byte codec, and segment descriptor stay available from
//! the crate root for compatibility. This module provides a coherent domain
//! hierarchy for new code and external integration tests.

pub mod codec;
pub mod record;
pub mod segment;

pub use codec::*;
pub use record::*;
pub use segment::*;
