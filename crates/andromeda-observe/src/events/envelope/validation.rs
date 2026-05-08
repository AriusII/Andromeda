use andromeda_error::AndromedaResult;

use crate::events::EventEnvelope;

mod event_contracts;
mod identity;

use super::{correlation, secret_safety};

pub(super) fn validate(envelope: &EventEnvelope) -> AndromedaResult<()> {
    identity::validate_identity(envelope)?;
    identity::validate_transition_payloads(envelope)?;
    event_contracts::validate_event_contracts(envelope)?;
    validate_common_guards(envelope)
}

fn validate_common_guards(envelope: &EventEnvelope) -> AndromedaResult<()> {
    secret_safety::validate(envelope)?;
    correlation::validate(envelope)
}
