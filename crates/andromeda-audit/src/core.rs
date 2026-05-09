use crate::TraceId;

/// Compatibility audit trace evidence.
///
/// This record is retained as review evidence only. It must not be used as the
/// source of authorization, catalog, storage, transaction, or recovery truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrace {
    pub trace_id: TraceId,
    pub actor: String,
    pub object: String,
    pub action: String,
}

impl AuditTrace {
    pub fn is_complete(&self) -> bool {
        !self.actor.trim().is_empty()
            && !self.object.trim().is_empty()
            && !self.action.trim().is_empty()
    }
}
