//! Unique identifier types for routing and correlation.
//!
//! This module defines stable, comparable identifier types used throughout
//! the Andromeda system for tracking requests, sessions, transactions, and
//! catalog objects.
//!
//! ## ID Types
//!
//! All regular ID types (`*Id`) wrap `u64`:
//! - **RequestId**: Correlates related messages for a single RPC call
//! - **SessionId**: Groups requests from a single client connection
//! - **TransactionId**: Unique reference for a transaction
//! - **CatalogObjectId**: References objects in the system catalog
//! - **CatalogVersion**: Increments when catalog objects change
//! - **DatabaseId**: References a database instance
//! - **InvocationId**: References a single invocation/execution
//! - **NamespaceId**: References a schema or namespace
//! - **ProcedureId**: References a procedure in the catalog
//!
//! ## ContractHash
//!
//! `ContractHash` is a 32-byte stable hash of a procedure's contract,
//! computed deterministically from the schema and policies.
//! Zero hash is reserved and indicates "no contract binding".
//!
//! Contract hashes enable:
//! - Protocol caching by hash
//! - Contract evolution tracking
//! - Change impact analysis

use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self::new(value)
            }
        }
    };
}

id_type!(CatalogVersion);
id_type!(CatalogObjectId);
id_type!(DatabaseId);
id_type!(InvocationId);
id_type!(NamespaceId);
id_type!(ProcedureId);
id_type!(RequestId);
id_type!(SessionId);
id_type!(TransactionId);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContractHash([u8; Self::LEN]);

impl ContractHash {
    pub const LEN: usize = 32;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> AndromedaResult<Self> {
        let bytes: [u8; Self::LEN] = bytes.try_into().map_err(|_| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "ContractHash must contain exactly 32 bytes",
            )
        })?;

        Ok(Self(bytes))
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn test_vector(byte: u8) -> Self {
        Self([byte; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }
}

impl Default for ContractHash {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Debug for ContractHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContractHash({self})")
    }
}

impl fmt::Display for ContractHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_hash_requires_exact_length() {
        assert_eq!(
            ContractHash::from_slice(&[1; 31]).unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
        assert!(ContractHash::from_slice(&[1; ContractHash::LEN]).is_ok());
    }

    #[test]
    fn typed_identifiers_preserve_values() {
        assert_eq!(RequestId::new(42).get(), 42);
        assert_eq!(SessionId::from(7).get(), 7);
    }
}
