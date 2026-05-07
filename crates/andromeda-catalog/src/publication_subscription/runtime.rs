use std::collections::BTreeMap;

use andromeda_error::AndromedaResult;
use andromeda_types::CatalogVersion;

use super::{
    CatalogPublicationReplayKey, CatalogPublicationReplayTerminalRecord, CatalogPublicationReport,
    CatalogPublicationSubscriptionReplayRecord, CatalogPublicationSubscriptionReplaySummary,
    CatalogSubscriberId, CatalogSubscriberKind, CatalogSubscriberRegistration,
    CatalogSubscriptionAcknowledgement, CatalogVisibleChangeAuditEvidence,
    catalog_publication_error, replay_publication_subscription_changes,
};
use crate::CatalogDurabilityMarker;

/// Visible publication state tracked by the subscription runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationRuntimeState {
    pub key: CatalogPublicationReplayKey,
    pub visible_catalog_version: CatalogVersion,
    pub expected_record_count: usize,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<CatalogDurabilityMarker>,
    pub audit_trace_id: String,
}

impl CatalogPublicationRuntimeState {
    fn from_publication(publication: &CatalogPublicationReport) -> AndromedaResult<Self> {
        publication.validate()?;
        Ok(Self {
            key: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            visible_catalog_version: publication.receipt.next_version,
            expected_record_count: publication.receipt.record_count,
            durable_lsn: publication.receipt.durable_lsn,
            durable_evidence_marker: publication.receipt.durable_evidence_marker,
            audit_trace_id: publication.audit_trace.trace_id.clone(),
        })
    }
}

/// Latest acknowledgement progress retained for a catalog subscriber.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSubscriberAckProgress {
    pub subscriber_id: CatalogSubscriberId,
    pub subscriber_kind: CatalogSubscriberKind,
    pub publication: CatalogPublicationReplayKey,
    pub acknowledged_version: CatalogVersion,
    pub expected_version: CatalogVersion,
    pub replayed_record_count: usize,
    pub expected_record_count: usize,
    pub durable_lsn_seen: Option<u64>,
    pub durable_evidence_marker_seen: Option<CatalogDurabilityMarker>,
    pub audit_trace_id: String,
}

impl CatalogSubscriberAckProgress {
    fn from_acknowledgement(
        acknowledgement: &CatalogSubscriptionAcknowledgement,
        subscriber_kind: CatalogSubscriberKind,
        publication: &CatalogPublicationReport,
    ) -> AndromedaResult<Self> {
        acknowledgement.validate_for_publication(publication)?;
        Ok(Self {
            subscriber_id: acknowledgement.subscriber_id.clone(),
            subscriber_kind,
            publication: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            acknowledged_version: acknowledgement.acknowledged_version,
            expected_version: publication.receipt.next_version,
            replayed_record_count: acknowledgement.replayed_record_count,
            expected_record_count: publication.receipt.record_count,
            durable_lsn_seen: acknowledgement.durable_lsn_seen,
            durable_evidence_marker_seen: acknowledgement.durable_evidence_marker_seen,
            audit_trace_id: acknowledgement.audit_trace_id.clone(),
        })
    }
}

/// Local proof that a HADR subscriber replay restored a catalog version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogHadrSubscriberReplayEvidence {
    pub subscriber_id: CatalogSubscriberId,
    pub restored_catalog_version: CatalogVersion,
    pub replayed_record_count: usize,
    pub expected_record_count: usize,
    pub durable_lsn_seen: Option<u64>,
    pub durable_evidence_marker_seen: Option<CatalogDurabilityMarker>,
    pub audit_trace_id: String,
}

impl CatalogHadrSubscriberReplayEvidence {
    fn from_progress(progress: &CatalogSubscriberAckProgress) -> AndromedaResult<Self> {
        if progress.subscriber_kind != CatalogSubscriberKind::HadrReplica {
            return catalog_publication_error(
                "catalog HADR replay evidence requires a HADR replica subscriber",
            );
        }
        Ok(Self {
            subscriber_id: progress.subscriber_id.clone(),
            restored_catalog_version: progress.acknowledged_version,
            replayed_record_count: progress.replayed_record_count,
            expected_record_count: progress.expected_record_count,
            durable_lsn_seen: progress.durable_lsn_seen,
            durable_evidence_marker_seen: progress.durable_evidence_marker_seen,
            audit_trace_id: progress.audit_trace_id.clone(),
        })
    }
}

/// Runtime registry for durable catalog publication/subscription acknowledgements.
#[derive(Debug, Clone, Default)]
pub struct CatalogPublicationSubscriberRegistry {
    subscribers: BTreeMap<CatalogSubscriberId, CatalogSubscriberKind>,
    publications: BTreeMap<CatalogPublicationReplayKey, CatalogPublicationReport>,
    publications_by_version:
        BTreeMap<CatalogPublicationRuntimeVersionKey, CatalogPublicationReplayKey>,
    acknowledgements: BTreeMap<CatalogSubscriberId, CatalogSubscriberAckProgress>,
    records: Vec<CatalogPublicationSubscriptionReplayRecord>,
}

impl CatalogPublicationSubscriberRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_subscriber(
        &mut self,
        registration: CatalogSubscriberRegistration,
    ) -> AndromedaResult<()> {
        if let Some(existing) = self.subscribers.get(&registration.subscriber_id) {
            if existing == &registration.kind {
                return Ok(());
            }
            return catalog_publication_error(
                "catalog subscriber registration conflicts with existing subscriber kind",
            );
        }
        self.subscribers
            .insert(registration.subscriber_id, registration.kind);
        Ok(())
    }

    pub fn record_visible_publication(
        &mut self,
        publication: CatalogPublicationReport,
        audit_evidence: CatalogVisibleChangeAuditEvidence,
        terminal: CatalogPublicationReplayTerminalRecord,
    ) -> AndromedaResult<CatalogPublicationRuntimeState> {
        terminal.validate_for_publication(&publication)?;
        let records = [
            CatalogPublicationSubscriptionReplayRecord::Terminal(terminal),
            CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                publication,
                audit_evidence,
            },
        ];
        self.apply_records(records)
    }

    pub fn acknowledge(
        &mut self,
        acknowledgement: CatalogSubscriptionAcknowledgement,
    ) -> AndromedaResult<CatalogSubscriberAckProgress> {
        self.apply_acknowledgement(acknowledgement)
    }

    pub fn subscriber_progress(
        &self,
        subscriber_id: &CatalogSubscriberId,
    ) -> Option<&CatalogSubscriberAckProgress> {
        self.acknowledgements.get(subscriber_id)
    }

    pub fn hadr_replay_evidence(
        &self,
        subscriber_id: &CatalogSubscriberId,
    ) -> AndromedaResult<Option<CatalogHadrSubscriberReplayEvidence>> {
        let Some(progress) = self.acknowledgements.get(subscriber_id) else {
            return Ok(None);
        };
        CatalogHadrSubscriberReplayEvidence::from_progress(progress).map(Some)
    }

    pub fn replay_summary(&self) -> AndromedaResult<CatalogPublicationSubscriptionReplaySummary> {
        replay_publication_subscription_changes(self.records.clone())
    }

    pub fn records(&self) -> &[CatalogPublicationSubscriptionReplayRecord] {
        &self.records
    }

    pub fn restore_from_replay(
        records: impl IntoIterator<Item = CatalogPublicationSubscriptionReplayRecord>,
        subscribers: impl IntoIterator<Item = CatalogSubscriberRegistration>,
    ) -> AndromedaResult<Self> {
        let records = records.into_iter().collect::<Vec<_>>();
        replay_publication_subscription_changes(records.clone())?;

        let mut registry = Self::new();
        for subscriber in subscribers {
            registry.register_subscriber(subscriber)?;
        }
        registry.apply_records(records)?;
        Ok(registry)
    }

    fn apply_records(
        &mut self,
        records: impl IntoIterator<Item = CatalogPublicationSubscriptionReplayRecord>,
    ) -> AndromedaResult<CatalogPublicationRuntimeState> {
        let records = records.into_iter().collect::<Vec<_>>();
        if records.is_empty() {
            return catalog_publication_error(
                "catalog publication subscriber registry requires replay records",
            );
        }

        let mut candidate = self.records.clone();
        candidate.extend(records.clone());
        replay_publication_subscription_changes(candidate)?;

        let mut latest_publication = None;
        for record in records {
            match record {
                CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                    publication,
                    audit_evidence,
                } => {
                    audit_evidence.validate_for_publication(&publication)?;
                    let state = self.apply_publication(publication.clone())?;
                    self.records.push(
                        CatalogPublicationSubscriptionReplayRecord::VisiblePublication {
                            publication,
                            audit_evidence,
                        },
                    );
                    latest_publication = Some(state);
                }
                CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                    acknowledgement,
                ) => {
                    self.apply_acknowledgement_after_replay_check(acknowledgement)?;
                }
                CatalogPublicationSubscriptionReplayRecord::Terminal(terminal) => {
                    self.records
                        .push(CatalogPublicationSubscriptionReplayRecord::Terminal(
                            terminal,
                        ));
                }
            }
        }

        match latest_publication {
            Some(state) => Ok(state),
            None => catalog_publication_error(
                "catalog publication subscriber registry requires a visible publication record",
            ),
        }
    }

    fn apply_publication(
        &mut self,
        publication: CatalogPublicationReport,
    ) -> AndromedaResult<CatalogPublicationRuntimeState> {
        let state = CatalogPublicationRuntimeState::from_publication(&publication)?;
        let version_key = CatalogPublicationRuntimeVersionKey::from_publication(&publication);

        if let Some(existing) = self.publications.get(&state.key) {
            if existing == &publication {
                return Ok(state);
            }
            return catalog_publication_error(
                "catalog publication subscriber registry saw conflicting publication identity",
            );
        }
        if let Some(existing_key) = self.publications_by_version.get(&version_key)
            && existing_key != &state.key
        {
            return catalog_publication_error(
                "catalog publication subscriber registry saw conflicting visible catalog version",
            );
        }

        self.publications_by_version.insert(version_key, state.key);
        self.publications.insert(state.key, publication);
        Ok(state)
    }

    fn apply_acknowledgement(
        &mut self,
        acknowledgement: CatalogSubscriptionAcknowledgement,
    ) -> AndromedaResult<CatalogSubscriberAckProgress> {
        let progress = self.build_ack_progress(&acknowledgement)?;
        let mut candidate = self.records.clone();
        candidate.push(
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement.clone(),
            ),
        );
        replay_publication_subscription_changes(candidate)?;
        self.commit_acknowledgement(acknowledgement, progress)
    }

    fn apply_acknowledgement_after_replay_check(
        &mut self,
        acknowledgement: CatalogSubscriptionAcknowledgement,
    ) -> AndromedaResult<CatalogSubscriberAckProgress> {
        let progress = self.build_ack_progress(&acknowledgement)?;
        self.commit_acknowledgement(acknowledgement, progress)
    }

    fn build_ack_progress(
        &self,
        acknowledgement: &CatalogSubscriptionAcknowledgement,
    ) -> AndromedaResult<CatalogSubscriberAckProgress> {
        let Some(subscriber_kind) = self
            .subscribers
            .get(&acknowledgement.subscriber_id)
            .copied()
        else {
            return catalog_publication_error(
                "catalog subscription acknowledgement requires a registered subscriber",
            );
        };

        let publication = self.publication_for_acknowledgement(acknowledgement)?;
        CatalogSubscriberAckProgress::from_acknowledgement(
            acknowledgement,
            subscriber_kind,
            publication,
        )
    }

    fn publication_for_acknowledgement(
        &self,
        acknowledgement: &CatalogSubscriptionAcknowledgement,
    ) -> AndromedaResult<&CatalogPublicationReport> {
        let version_key =
            CatalogPublicationRuntimeVersionKey::from_acknowledgement(acknowledgement);
        if let Some(publication_key) = self.publications_by_version.get(&version_key).copied() {
            let Some(publication) = self.publications.get(&publication_key) else {
                return catalog_publication_error(
                    "catalog subscription acknowledgement publication index is inconsistent",
                );
            };
            return Ok(publication);
        }

        if let Some(publication) = self.publications.values().find(|publication| {
            publication.receipt.database_id == acknowledgement.database_id
                && publication.receipt.namespace_id == acknowledgement.namespace_id
        }) {
            acknowledgement.validate_for_publication(publication)?;
        }

        catalog_publication_error(
            "catalog subscription acknowledgement requires a visible publication",
        )
    }

    fn commit_acknowledgement(
        &mut self,
        acknowledgement: CatalogSubscriptionAcknowledgement,
        progress: CatalogSubscriberAckProgress,
    ) -> AndromedaResult<CatalogSubscriberAckProgress> {
        if let Some(existing) = self.acknowledgements.get(&acknowledgement.subscriber_id) {
            if existing.acknowledged_version > progress.acknowledged_version {
                return catalog_publication_error(
                    "catalog subscription acknowledgement must not move a subscriber backwards",
                );
            }
            if existing.acknowledged_version == progress.acknowledged_version {
                if existing == &progress {
                    return Ok(existing.clone());
                }
                return catalog_publication_error(
                    "catalog subscription acknowledgement conflicts with existing subscriber progress",
                );
            }
        }

        if progress.replayed_record_count != progress.expected_record_count
            || progress.acknowledged_version != progress.expected_version
        {
            return catalog_publication_error(
                "catalog subscription acknowledgement progress must match expected version and record count",
            );
        }

        self.records.push(
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ),
        );
        self.acknowledgements
            .insert(progress.subscriber_id.clone(), progress.clone());
        Ok(progress)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CatalogPublicationRuntimeVersionKey {
    database_id: u64,
    namespace_id: u64,
    catalog_version: u64,
}

impl CatalogPublicationRuntimeVersionKey {
    fn from_publication(publication: &CatalogPublicationReport) -> Self {
        Self {
            database_id: publication.receipt.database_id.get(),
            namespace_id: publication.receipt.namespace_id.get(),
            catalog_version: publication.receipt.next_version.get(),
        }
    }

    fn from_acknowledgement(acknowledgement: &CatalogSubscriptionAcknowledgement) -> Self {
        Self {
            database_id: acknowledgement.database_id.get(),
            namespace_id: acknowledgement.namespace_id.get(),
            catalog_version: acknowledgement.acknowledged_version.get(),
        }
    }
}
