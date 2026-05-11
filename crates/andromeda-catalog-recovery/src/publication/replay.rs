use std::collections::BTreeMap;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::CatalogVersion;

use super::{
    CatalogPublicationReceiptView, CatalogPublicationReport, CatalogSubscriberIdentity,
    CatalogSubscriptionAcknowledgement, CatalogVisibleChangeAuditEvidence,
    catalog_recovery_publication_error, require_equal,
    validate_subscription_acknowledgement_for_publication,
    validate_visible_change_audit_for_publication,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationReplayTerminalOutcome {
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayRecordKind {
    Publication,
    SubscriptionAcknowledgement,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogPublicationReplayKey {
    pub batch_id: u64,
    pub database_id: u64,
    pub namespace_id: u64,
    pub previous_version: u64,
    pub next_version: u64,
}

impl CatalogPublicationReplayKey {
    pub fn from_receipt<TReceipt>(receipt: &TReceipt) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            batch_id: receipt.batch_id_value(),
            database_id: receipt.database_id().get(),
            namespace_id: receipt.namespace_id().get(),
            previous_version: receipt.previous_version().get(),
            next_version: receipt.next_version().get(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogSubscriptionReplayKey {
    pub subscriber_id: String,
    pub database_id: u64,
    pub namespace_id: u64,
    pub acknowledged_version: u64,
}

impl CatalogSubscriptionReplayKey {
    pub fn from_acknowledgement<TSubscriberId, TDurabilityMarker>(
        acknowledgement: &CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker>,
    ) -> Self
    where
        TSubscriberId: CatalogSubscriberIdentity,
    {
        Self {
            subscriber_id: acknowledgement
                .subscriber_id
                .catalog_subscriber_id()
                .to_owned(),
            database_id: acknowledgement.database_id.get(),
            namespace_id: acknowledgement.namespace_id.get(),
            acknowledged_version: acknowledgement.acknowledged_version.get(),
        }
    }
}

/// Durable terminal evidence observed while replaying a publication stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReplayTerminalRecord<TDurabilityMarker> {
    pub key: CatalogPublicationReplayKey,
    pub outcome: CatalogPublicationReplayTerminalOutcome,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<TDurabilityMarker>,
    pub record_count: usize,
    pub audit_trace_id: String,
}

impl<TDurabilityMarker> CatalogPublicationReplayTerminalRecord<TDurabilityMarker>
where
    TDurabilityMarker: Clone + PartialEq + Eq,
{
    pub fn committed_for_publication<TReceipt>(
        publication: &CatalogPublicationReport<TReceipt>,
    ) -> Self
    where
        TReceipt: CatalogPublicationReceiptView<DurabilityMarker = TDurabilityMarker>,
    {
        let durable_evidence = publication.durable_evidence();
        Self {
            key: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            outcome: CatalogPublicationReplayTerminalOutcome::Committed,
            durable_lsn: durable_evidence.durable_lsn,
            durable_evidence_marker: durable_evidence.durable_evidence_marker,
            record_count: publication.receipt.record_count(),
            audit_trace_id: publication.audit_trace.trace_id.clone(),
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.key.batch_id == 0
            || self.key.database_id == 0
            || self.key.namespace_id == 0
            || self.key.previous_version == 0
            || self.key.next_version <= self.key.previous_version
        {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal identity must describe a monotonic nonzero publication",
            );
        }
        let expected_next = self.key.previous_version.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "catalog publication replay terminal version advance overflowed",
            )
        })?;
        if self.key.next_version != expected_next {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal must advance by exactly one catalog version",
            );
        }
        if self.record_count == 0 {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal record count must not be zero",
            );
        }
        if self.audit_trace_id.trim().is_empty() {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal audit trace id must not be empty",
            );
        }
        if self.durable_lsn == Some(0) {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal durable LSN must not be zero",
            );
        }
        if self.durable_lsn.is_none() && self.durable_evidence_marker.is_none() {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal must carry durable LSN or marker evidence",
            );
        }
        Ok(())
    }

    pub fn validate_for_publication<TReceipt>(
        &self,
        publication: &CatalogPublicationReport<TReceipt>,
    ) -> AndromedaResult<()>
    where
        TReceipt: CatalogPublicationReceiptView<DurabilityMarker = TDurabilityMarker>,
    {
        self.validate()?;
        let durable_evidence = publication.durable_evidence();
        if self.outcome != CatalogPublicationReplayTerminalOutcome::Committed {
            return catalog_recovery_publication_error(
                "catalog publication replay terminal must be committed before applying visible publication",
            );
        }
        require_equal(
            &self.key,
            &CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            "catalog publication replay terminal identity must match publication",
        )?;
        require_equal(
            &self.durable_lsn,
            &durable_evidence.durable_lsn,
            "catalog publication replay terminal durable LSN must match publication",
        )?;
        require_equal(
            &self.durable_evidence_marker.as_ref(),
            &durable_evidence.durable_evidence_marker.as_ref(),
            "catalog publication replay terminal durable marker must match publication",
        )?;
        require_equal(
            &self.record_count,
            &publication.receipt.record_count(),
            "catalog publication replay terminal record count must match publication",
        )?;
        require_equal(
            &self.audit_trace_id,
            &publication.audit_trace.trace_id,
            "catalog publication replay terminal audit trace id must match publication",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayEvidence {
    PublicationApplied {
        key: CatalogPublicationReplayKey,
        audit_trace_id: String,
    },
    SubscriptionAcknowledged {
        key: CatalogSubscriptionReplayKey,
        publication: CatalogPublicationReplayKey,
    },
    TerminalObserved {
        key: CatalogPublicationReplayKey,
        outcome: CatalogPublicationReplayTerminalOutcome,
    },
    DuplicateIgnored {
        record_kind: CatalogPublicationSubscriptionReplayRecordKind,
    },
}

#[allow(
    clippy::large_enum_variant,
    reason = "Replay records intentionally carry complete catalog publication evidence for fail-closed comparison."
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayRecord<TReceipt, TSubscriberId>
where
    TReceipt: CatalogPublicationReceiptView,
{
    VisiblePublication {
        publication: CatalogPublicationReport<TReceipt>,
        audit_evidence: CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>,
    },
    SubscriptionAcknowledgement(
        CatalogSubscriptionAcknowledgement<TSubscriberId, TReceipt::DurabilityMarker>,
    ),
    Terminal(CatalogPublicationReplayTerminalRecord<TReceipt::DurabilityMarker>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationSubscriptionReplaySummary {
    pub applied_publication_count: usize,
    pub acknowledged_subscription_count: usize,
    pub terminal_record_count: usize,
    pub duplicate_record_count: usize,
    pub audit_before_visible_change_count: usize,
    pub final_visible_catalog_version: Option<CatalogVersion>,
    pub evidence: Vec<CatalogPublicationSubscriptionReplayEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedPublication<TReceipt>
where
    TReceipt: CatalogPublicationReceiptView,
{
    publication: CatalogPublicationReport<TReceipt>,
    audit_evidence: CatalogVisibleChangeAuditEvidence<TReceipt::DurabilityMarker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PublishedVersionKey {
    database_id: u64,
    namespace_id: u64,
    catalog_version: u64,
}

impl PublishedVersionKey {
    pub(crate) fn from_publication<TReceipt>(
        publication: &CatalogPublicationReport<TReceipt>,
    ) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            database_id: publication.receipt.database_id().get(),
            namespace_id: publication.receipt.namespace_id().get(),
            catalog_version: publication.receipt.next_version().get(),
        }
    }

    pub(crate) fn from_acknowledgement<TSubscriberId, TDurabilityMarker>(
        acknowledgement: &CatalogSubscriptionAcknowledgement<TSubscriberId, TDurabilityMarker>,
    ) -> Self {
        Self {
            database_id: acknowledgement.database_id.get(),
            namespace_id: acknowledgement.namespace_id.get(),
            catalog_version: acknowledgement.acknowledged_version.get(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PublishedScopeKey {
    database_id: u64,
    namespace_id: u64,
}

impl PublishedScopeKey {
    fn from_publication<TReceipt>(publication: &CatalogPublicationReport<TReceipt>) -> Self
    where
        TReceipt: CatalogPublicationReceiptView,
    {
        Self {
            database_id: publication.receipt.database_id().get(),
            namespace_id: publication.receipt.namespace_id().get(),
        }
    }
}

pub fn replay_publication_subscription_changes<TReceipt, TSubscriberId>(
    records: impl IntoIterator<
        Item = CatalogPublicationSubscriptionReplayRecord<TReceipt, TSubscriberId>,
    >,
) -> AndromedaResult<CatalogPublicationSubscriptionReplaySummary>
where
    TReceipt: CatalogPublicationReceiptView + Clone + PartialEq + Eq,
    TSubscriberId: CatalogSubscriberIdentity + Clone + PartialEq + Eq,
{
    let mut publications =
        BTreeMap::<CatalogPublicationReplayKey, AppliedPublication<TReceipt>>::new();
    let mut publications_by_version =
        BTreeMap::<PublishedVersionKey, CatalogPublicationReplayKey>::new();
    let mut visible_versions = BTreeMap::<PublishedScopeKey, u64>::new();
    let mut acknowledgements = BTreeMap::<
        CatalogSubscriptionReplayKey,
        CatalogSubscriptionAcknowledgement<TSubscriberId, TReceipt::DurabilityMarker>,
    >::new();
    let mut terminals = BTreeMap::<
        CatalogPublicationReplayKey,
        CatalogPublicationReplayTerminalRecord<TReceipt::DurabilityMarker>,
    >::new();
    let mut duplicate_record_count = 0usize;
    let mut audit_before_visible_change_count = 0usize;
    let mut evidence = Vec::new();

    for record in records {
        match record {
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication,
                audit_evidence,
            } => {
                publication.validate()?;
                validate_visible_change_audit_for_publication(&audit_evidence, &publication)?;
                let key = CatalogPublicationReplayKey::from_receipt(&publication.receipt);

                let Some(terminal) = terminals.get(&key) else {
                    return catalog_recovery_publication_error(
                        "catalog visible publication replay requires a committed durable terminal record",
                    );
                };
                terminal.validate_for_publication(&publication)?;

                let applied = AppliedPublication {
                    publication,
                    audit_evidence,
                };

                if let Some(existing) = publications.get(&key) {
                    if existing == &applied {
                        duplicate_record_count += 1;
                        evidence.push(
                            CatalogPublicationSubscriptionReplayEvidence::DuplicateIgnored {
                                record_kind:
                                    CatalogPublicationSubscriptionReplayRecordKind::Publication,
                            },
                        );
                        continue;
                    }
                    return catalog_recovery_publication_error(
                        "conflicting catalog publication replay record for the same publication identity",
                    );
                }

                let visible_key = PublishedVersionKey::from_publication(&applied.publication);
                if let Some(existing_key) = publications_by_version.get(&visible_key)
                    && existing_key != &key
                {
                    return catalog_recovery_publication_error(
                        "conflicting catalog publication replay record for the same visible catalog version",
                    );
                }

                let scope_key = PublishedScopeKey::from_publication(&applied.publication);
                if let Some(current_version) = visible_versions.get(&scope_key)
                    && applied.publication.receipt.previous_version().get() != *current_version
                {
                    return catalog_recovery_publication_error(
                        "catalog publication replay must preserve catalog version ordering",
                    );
                }

                publications_by_version.insert(visible_key, key);
                visible_versions
                    .insert(scope_key, applied.publication.receipt.next_version().get());
                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::PublicationApplied {
                        key,
                        audit_trace_id: applied.publication.audit_trace.trace_id.clone(),
                    },
                );
                publications.insert(key, applied);
                audit_before_visible_change_count += 1;
            },
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ) => {
                let version_key = PublishedVersionKey::from_acknowledgement(&acknowledgement);
                let Some(publication_key) = publications_by_version.get(&version_key).copied()
                else {
                    if let Some(publication) = publications.values().find(|publication| {
                        publication.publication.receipt.database_id() == acknowledgement.database_id
                            && publication.publication.receipt.namespace_id()
                                == acknowledgement.namespace_id
                    }) {
                        validate_subscription_acknowledgement_for_publication(
                            &acknowledgement,
                            &publication.publication,
                        )?;
                    }
                    return catalog_recovery_publication_error(
                        "catalog subscription acknowledgement replay requires a visible publication",
                    );
                };
                let Some(publication) = publications.get(&publication_key) else {
                    return catalog_recovery_publication_error(
                        "catalog subscription acknowledgement replay publication index is inconsistent",
                    );
                };
                validate_subscription_acknowledgement_for_publication(
                    &acknowledgement,
                    &publication.publication,
                )?;

                let acknowledgement_key =
                    CatalogSubscriptionReplayKey::from_acknowledgement(&acknowledgement);
                if let Some(existing) = acknowledgements.get(&acknowledgement_key) {
                    if existing == &acknowledgement {
                        duplicate_record_count += 1;
                        evidence.push(
                            CatalogPublicationSubscriptionReplayEvidence::DuplicateIgnored {
                                record_kind:
                                    CatalogPublicationSubscriptionReplayRecordKind::SubscriptionAcknowledgement,
                            },
                        );
                        continue;
                    }
                    return catalog_recovery_publication_error(
                        "conflicting catalog subscription acknowledgement replay record",
                    );
                }

                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::SubscriptionAcknowledged {
                        key: acknowledgement_key.clone(),
                        publication: publication_key,
                    },
                );
                acknowledgements.insert(acknowledgement_key, acknowledgement);
            },
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal) => {
                terminal.validate()?;
                if let Some(existing) = terminals.get(&terminal.key) {
                    if existing == &terminal {
                        duplicate_record_count += 1;
                        evidence.push(
                            CatalogPublicationSubscriptionReplayEvidence::DuplicateIgnored {
                                record_kind:
                                    CatalogPublicationSubscriptionReplayRecordKind::Terminal,
                            },
                        );
                        continue;
                    }
                    return catalog_recovery_publication_error(
                        "conflicting catalog publication replay terminal record",
                    );
                }

                if let Some(applied) = publications.get(&terminal.key) {
                    terminal.validate_for_publication(&applied.publication)?;
                }

                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::TerminalObserved {
                        key: terminal.key,
                        outcome: terminal.outcome,
                    },
                );
                terminals.insert(terminal.key, terminal);
            },
        }
    }

    let final_visible_catalog_version = publications
        .values()
        .map(|publication| publication.publication.receipt.next_version())
        .max_by_key(|version| version.get());

    Ok(CatalogPublicationSubscriptionReplaySummary {
        applied_publication_count: publications.len(),
        acknowledged_subscription_count: acknowledgements.len(),
        terminal_record_count: terminals.len(),
        duplicate_record_count,
        audit_before_visible_change_count,
        final_visible_catalog_version,
        evidence,
    })
}
