//! Storage compatibility path for manifest publication recovery.
//!
//! Only `DatabaseManifest` remains here because recovery contract support still
//! imports this historical path. Other publication-domain contracts are
//! available from their owners: manifest snapshots from `andromeda_manifest`
//! and cold publication validation from the storage crate root.

pub use crate::DatabaseManifest;
