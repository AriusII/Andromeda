//! Storage layout domain facade.
//!
//! The flat root exports remain stable for compatibility, while this module
//! groups page, extent, segment, and cold-store contracts by storage domain for
//! external callers and integration tests.

pub mod cold;
pub mod extent;
pub mod io_budget;
pub mod page;
pub mod placement;
pub mod segment;

pub use cold::*;
pub use extent::*;
pub use io_budget::*;
pub use page::*;
pub use placement::*;
pub use segment::*;
