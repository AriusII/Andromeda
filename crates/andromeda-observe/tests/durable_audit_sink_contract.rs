#[path = "durable_audit_sink_contract/support.rs"]
mod support;

#[path = "durable_audit_sink_contract/append_replay.rs"]
mod append_replay;
#[path = "durable_audit_sink_contract/compaction_anchor.rs"]
mod compaction_anchor;
#[path = "durable_audit_sink_contract/corruption_truncation.rs"]
mod corruption_truncation;
#[path = "durable_audit_sink_contract/policy_evidence.rs"]
mod policy_evidence;
#[path = "durable_audit_sink_contract/retention.rs"]
mod retention;
