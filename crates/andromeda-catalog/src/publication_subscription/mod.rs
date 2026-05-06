//! Typed catalog publication/subscription contract surface.
//!
//! This module is **Administration/HA only**.  It does not expose application
//! traffic, SQL, gRPC, JSON, storage I/O, or a subscriber runtime.  The types
//! here document and validate the bounded evidence that must travel between a
//! durable catalog publication producer and administrative/HA subscribers.

mod acknowledgement;
mod audience;
mod audit;
mod helpers;
mod invalidation;
mod published_object;
mod recovery_replay;
mod report;
mod subscriber;

pub use acknowledgement::*;
pub use audience::*;
pub use audit::*;
pub use invalidation::*;
pub use published_object::*;
pub use recovery_replay::*;
pub use report::*;
pub use subscriber::*;

use helpers::{catalog_publication_error, validate_receipt};
