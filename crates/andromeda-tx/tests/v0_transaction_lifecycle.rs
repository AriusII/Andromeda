//! V0 transaction lifecycle contracts split by state-machine, MVCC visibility,
//! snapshot validation, and manager-driven durable status transitions.

#[path = "v0_transaction_lifecycle/fixtures.rs"]
mod fixtures;
#[path = "v0_transaction_lifecycle/manager_visibility.rs"]
mod manager_visibility;
#[path = "v0_transaction_lifecycle/mvcc_visibility.rs"]
mod mvcc_visibility;
#[path = "v0_transaction_lifecycle/snapshot_validation.rs"]
mod snapshot_validation;
#[path = "v0_transaction_lifecycle/state_machine_durability.rs"]
mod state_machine_durability;
