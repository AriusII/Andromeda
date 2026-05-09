//! Durable HADR membership storage.
//!
//! The store owns only cluster membership and role/epoch metadata. It does not
//! participate in WAL recovery, page storage, or CLI dry-run contracts.

mod file_backed;
mod model;
mod store;

pub use file_backed::FileBackedHadrMembershipStore;
pub use model::{HadrMembershipNode, HadrMembershipRecord, HadrMembershipSnapshot};
pub use store::HadrMembershipStore;

fn io_error(action: &str, err: std::io::Error) -> andromeda_error::AndromedaError {
    andromeda_error::AndromedaError::new(
        andromeda_error::AndromedaErrorKind::Storage,
        format!("{action}: {err}"),
    )
}

fn storage_error(message: &'static str) -> andromeda_error::AndromedaError {
    andromeda_error::AndromedaError::new(andromeda_error::AndromedaErrorKind::Storage, message)
}
