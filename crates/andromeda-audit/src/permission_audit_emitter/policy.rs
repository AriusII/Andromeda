use super::evidence::AuditEmissionEventFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEmissionPolicy {
    fail_closed_when_sink_unavailable: bool,
    require_durable_wal_evidence: bool,
    expected_event_family: Option<AuditEmissionEventFamily>,
}

impl AuditEmissionPolicy {
    pub const fn fail_closed() -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: false,
            expected_event_family: None,
        }
    }

    pub const fn fail_closed_with_durable_wal() -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: true,
            expected_event_family: None,
        }
    }

    pub fn fail_closed_for_visible_decision(
        expected_event_family: impl Into<AuditEmissionEventFamily>,
    ) -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: true,
            expected_event_family: Some(expected_event_family.into()),
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
            fail_closed_when_sink_unavailable: false,
            require_durable_wal_evidence: false,
            expected_event_family: None,
        }
    }

    pub const fn requires_available_sink(self) -> bool {
        self.fail_closed_when_sink_unavailable
    }

    pub const fn requires_durable_wal_evidence(self) -> bool {
        self.require_durable_wal_evidence
    }

    pub const fn expected_event_family(self) -> Option<AuditEmissionEventFamily> {
        self.expected_event_family
    }
}

impl Default for AuditEmissionPolicy {
    fn default() -> Self {
        Self::fail_closed()
    }
}
