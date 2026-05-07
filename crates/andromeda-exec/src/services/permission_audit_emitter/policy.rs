use andromeda_observe::DurableAuditEventFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEmissionPolicy {
    fail_closed_when_sink_unavailable: bool,
    require_durable_wal_evidence: bool,
    expected_event_family: Option<DurableAuditEventFamily>,
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

    pub const fn fail_closed_for_visible_decision(
        expected_event_family: DurableAuditEventFamily,
    ) -> Self {
        Self {
            fail_closed_when_sink_unavailable: true,
            require_durable_wal_evidence: true,
            expected_event_family: Some(expected_event_family),
        }
    }

    pub const fn allow_unavailable_sink_for_tests() -> Self {
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

    pub const fn expected_event_family(self) -> Option<DurableAuditEventFamily> {
        self.expected_event_family
    }
}

impl Default for AuditEmissionPolicy {
    fn default() -> Self {
        Self::fail_closed()
    }
}
