#![forbid(unsafe_code)]

#[path = "orphan_source_invariants/dependency_doctrine.rs"]
mod dependency_doctrine;
#[path = "orphan_source_invariants/dependency_manifest.rs"]
mod dependency_manifest;
#[path = "orphan_source_invariants/source_inventory.rs"]
mod source_inventory;
#[path = "orphan_source_invariants/source_inventory_contract.rs"]
mod source_inventory_contract;
#[path = "orphan_source_invariants/support.rs"]
mod support;
