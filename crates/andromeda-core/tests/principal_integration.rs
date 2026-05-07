#[allow(dead_code)]
#[path = "support/principal_fixtures.rs"]
mod principal_fixtures;

#[path = "principal_integration/certificate_identity.rs"]
mod certificate_identity;

#[path = "principal_integration/denial_reasons.rs"]
mod denial_reasons;

#[path = "principal_integration/disabled_revoked_states.rs"]
mod disabled_revoked_states;

#[path = "principal_integration/policy_evidence.rs"]
mod policy_evidence;

#[path = "principal_integration/principal_registry.rs"]
mod principal_registry;

#[path = "principal_integration/resource_policy_gates.rs"]
mod resource_policy_gates;
