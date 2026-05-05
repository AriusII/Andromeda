//! Transaction-local durable log sequence number contract.
//!
//! The transaction crate must not depend on the storage crate. This LSN type is
//! the boundary value used by transaction/WAL adapters; storage implementations
//! convert at the adapter boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lsn(u64);

impl Lsn {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}
