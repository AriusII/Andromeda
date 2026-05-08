use andromeda_observe::DurableAuditEventFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEmissionPolicy {
    inner: andromeda_audit::AuditEmissionPolicy,
    expected_event_family: Option<DurableAuditEventFamily>,
}

impl AuditEmissionPolicy {
    pub const fn fail_closed() -> Self {
        Self {
            inner: andromeda_audit::AuditEmissionPolicy::fail_closed(),
            expected_event_family: None,
        }
    }

    pub const fn fail_closed_with_durable_wal() -> Self {
        Self {
            inner: andromeda_audit::AuditEmissionPolicy::fail_closed_with_durable_wal(),
            expected_event_family: None,
        }
    }

    pub const fn fail_closed_for_visible_decision(
        expected_event_family: DurableAuditEventFamily,
    ) -> Self {
        Self {
            inner: andromeda_audit::AuditEmissionPolicy::fail_closed_for_visible_decision(
                durable_family_to_audit_family(expected_event_family),
            ),
            expected_event_family: Some(expected_event_family),
        }
    }

    /// Test-support policy for validation-only tests that do not model audit durability.
    /// Visible security decisions must use a fail-closed durable policy instead.
    pub const fn allow_unavailable_sink_for_explicit_test_support() -> Self {
        Self::test_support_allow_unavailable_sink()
    }

    #[doc(hidden)]
    pub const fn allow_unavailable_sink_for_tests() -> Self {
        Self::test_support_allow_unavailable_sink()
    }

    const fn test_support_allow_unavailable_sink() -> Self {
        Self {
            inner: andromeda_audit::AuditEmissionPolicy::allow_unavailable_sink_for_tests(),
            expected_event_family: None,
        }
    }

    pub const fn requires_available_sink(self) -> bool {
        self.inner.requires_available_sink()
    }

    pub const fn requires_durable_wal_evidence(self) -> bool {
        self.inner.requires_durable_wal_evidence()
    }

    pub const fn expected_event_family(self) -> Option<DurableAuditEventFamily> {
        self.expected_event_family
    }

    pub(crate) const fn into_audit(self) -> andromeda_audit::AuditEmissionPolicy {
        self.inner
    }
}

impl Default for AuditEmissionPolicy {
    fn default() -> Self {
        Self::fail_closed()
    }
}

pub(crate) const fn durable_family_to_audit_family(
    family: DurableAuditEventFamily,
) -> andromeda_audit::AuditEmissionEventFamily {
    match family {
        DurableAuditEventFamily::SecurityDecision => {
            andromeda_audit::AuditEmissionEventFamily::SecurityDecision
        }
        DurableAuditEventFamily::AdminDecision => {
            andromeda_audit::AuditEmissionEventFamily::AdminDecision
        }
        DurableAuditEventFamily::AdmissionDecision => {
            andromeda_audit::AuditEmissionEventFamily::AdmissionDecision
        }
        DurableAuditEventFamily::CatalogDecision => {
            andromeda_audit::AuditEmissionEventFamily::CatalogDecision
        }
        DurableAuditEventFamily::HadrDecision => {
            andromeda_audit::AuditEmissionEventFamily::HadrDecision
        }
        DurableAuditEventFamily::BackupDecision => {
            andromeda_audit::AuditEmissionEventFamily::BackupDecision
        }
        DurableAuditEventFamily::RestoreDecision => {
            andromeda_audit::AuditEmissionEventFamily::RestoreDecision
        }
        DurableAuditEventFamily::ForensicDecision => {
            andromeda_audit::AuditEmissionEventFamily::ForensicDecision
        }
        DurableAuditEventFamily::RecoveryDecision => {
            andromeda_audit::AuditEmissionEventFamily::RecoveryDecision
        }
        DurableAuditEventFamily::GenericAudit => {
            andromeda_audit::AuditEmissionEventFamily::GenericAudit
        }
    }
}
