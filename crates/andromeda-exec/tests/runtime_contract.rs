#[path = "runtime_contract/common.rs"]
mod common;

#[path = "runtime_contract/authorization.rs"]
mod authorization;
#[path = "runtime_contract/lifecycle.rs"]
mod lifecycle;
#[path = "runtime_contract/procedure_only_dispatch.rs"]
mod procedure_only_dispatch;
#[path = "runtime_contract/result_stream.rs"]
mod result_stream;
#[path = "runtime_contract/rollback.rs"]
mod rollback;
#[path = "runtime_contract/wal_evidence.rs"]
mod wal_evidence;
