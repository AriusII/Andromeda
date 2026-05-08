use andromeda_error::AndromedaResult;

use crate::events::{EventEnvelope, EventId, observe_error};

use super::InMemoryEventSequence;

impl InMemoryEventSequence {
    pub(super) fn validate_event_id_order(&self, event_id: EventId) -> AndromedaResult<()> {
        if self
            .last_event_id
            .is_some_and(|last_event_id| event_id <= last_event_id)
        {
            return Err(observe_error(
                "procedure lifecycle event sequence requires strictly increasing event_id values",
            ));
        }

        Ok(())
    }

    pub(super) fn validate_stable_procedure_correlation(
        &self,
        event: &EventEnvelope,
    ) -> AndromedaResult<()> {
        let correlation = event.correlation;
        if !correlation.has_request_session()
            || !correlation.has_contract_catalog()
            || correlation
                .catalog_object_id
                .is_none_or(|catalog_object_id| catalog_object_id.get() == 0)
        {
            return Err(observe_error(
                "procedure lifecycle events require non-zero request_id, session_id, contract_hash, catalog_version, and catalog_object_id correlation",
            ));
        }

        Self::validate_anchor("request_id", self.request_id, correlation.request_id)?;
        Self::validate_anchor("session_id", self.session_id, correlation.session_id)?;
        Self::validate_anchor(
            "contract_hash",
            self.contract_hash,
            correlation.contract_hash,
        )?;
        Self::validate_anchor(
            "catalog_version",
            self.catalog_version,
            correlation.catalog_version,
        )?;
        Self::validate_anchor(
            "catalog_object_id",
            self.catalog_object_id,
            correlation.catalog_object_id,
        )
    }

    fn validate_anchor<T: Copy + Eq>(
        label: &str,
        expected: Option<T>,
        observed: Option<T>,
    ) -> AndromedaResult<()> {
        match expected {
            Some(expected) if observed != Some(expected) => {
                return Err(observe_error(format!(
                    "procedure lifecycle {label} correlation must remain stable across the sequence",
                )));
            }
            _ => {}
        }

        Ok(())
    }
}
