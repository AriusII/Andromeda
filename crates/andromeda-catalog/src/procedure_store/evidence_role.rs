/// Closed Procedure Store evidence role taxonomy.
///
/// The role is deliberately explicit so callers cannot conflate critical
/// invocation decisions with observed runtime feedback. Decision records
/// document an authoritative decision already made by the relevant engine;
/// runtime records and counters remain observed evidence only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcedureStoreEvidenceRole {
    kind: ProcedureStoreEvidenceRoleKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ProcedureStoreEvidenceRoleKind {
    AuthoritativeDecision,
    ObservedFeedback,
}

impl ProcedureStoreEvidenceRole {
    pub const VARIANT_COUNT: usize = 2;

    pub(crate) const fn authoritative_decision() -> Self {
        Self {
            kind: ProcedureStoreEvidenceRoleKind::AuthoritativeDecision,
        }
    }

    pub(crate) const fn observed_feedback() -> Self {
        Self {
            kind: ProcedureStoreEvidenceRoleKind::ObservedFeedback,
        }
    }

    pub const fn as_tag(self) -> u8 {
        match self.kind {
            ProcedureStoreEvidenceRoleKind::AuthoritativeDecision => 0x01,
            ProcedureStoreEvidenceRoleKind::ObservedFeedback => 0x02,
        }
    }

    pub const fn is_authoritative_decision(self) -> bool {
        matches!(
            self.kind,
            ProcedureStoreEvidenceRoleKind::AuthoritativeDecision
        )
    }

    pub const fn is_observed_feedback(self) -> bool {
        matches!(self.kind, ProcedureStoreEvidenceRoleKind::ObservedFeedback)
    }

    pub const fn can_select_plan_alone(self) -> bool {
        false
    }
}
