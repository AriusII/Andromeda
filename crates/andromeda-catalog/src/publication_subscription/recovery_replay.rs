use std::collections::BTreeMap;

pub use andromeda_catalog_recovery::{
    CatalogPublicationReplayTerminalOutcome, CatalogPublicationSubscriptionReplayRecordKind,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::CatalogVersion;

use super::{
    CatalogPublicationReport, CatalogSubscriptionAcknowledgement,
    CatalogVisibleChangeAuditEvidence, catalog_publication_error, require_equal,
    validate_subscription_acknowledgement_for_publication,
    validate_visible_change_audit_for_publication,
};
use crate::{CatalogDurabilityMarker, CatalogPublicationReceipt};

/// Recovery/replay expectations attached to a durable publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogRecoveryReplayExpectation {
    pub starting_version: CatalogVersion,
    pub target_version: CatalogVersion,
    pub required_record_count: usize,
    pub require_exact_commit_boundary: bool,
}

impl CatalogRecoveryReplayExpectation {
    pub const fn from_receipt(receipt: &CatalogPublicationReceipt) -> Self {
        Self {
            starting_version: receipt.previous_version,
            target_version: receipt.next_version,
            required_record_count: receipt.record_count,
            require_exact_commit_boundary: true,
        }
    }

    pub fn validate_for_receipt(&self, receipt: &CatalogPublicationReceipt) -> AndromedaResult<()> {
        require_equal(
            &self.starting_version,
            &receipt.previous_version,
            "recovery replay expectation starting version must match publication receipt",
        )?;
        require_equal(
            &self.target_version,
            &receipt.next_version,
            "recovery replay expectation target version must match publication receipt",
        )?;
        require_equal(
            &self.required_record_count,
            &receipt.record_count,
            "recovery replay expectation record count must match publication receipt",
        )?;
        let expected_next = self.starting_version.get().checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "recovery replay expectation version advance overflowed",
            )
        })?;
        if self.target_version.get() != expected_next {
            return catalog_publication_error(
                "recovery replay expectation must advance by exactly one catalog version",
            );
        }
        if !self.require_exact_commit_boundary {
            return catalog_publication_error(
                "catalog recovery replay must require exact commit boundary matching",
            );
        }
        Ok(())
    }
}

/// Stable identity for publication/subscription recovery replay records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogPublicationReplayKey {
    pub batch_id: u64,
    pub database_id: u64,
    pub namespace_id: u64,
    pub previous_version: u64,
    pub next_version: u64,
}

impl CatalogPublicationReplayKey {
    pub fn from_receipt(receipt: &CatalogPublicationReceipt) -> Self {
        Self {
            batch_id: receipt.batch_id.get(),
            database_id: receipt.database_id.get(),
            namespace_id: receipt.namespace_id.get(),
            previous_version: receipt.previous_version.get(),
            next_version: receipt.next_version.get(),
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
    pub fn from_acknowledgement(acknowledgement: &CatalogSubscriptionAcknowledgement) -> Self {
        Self {
            subscriber_id: acknowledgement.subscriber_id.as_str().to_owned(),
            database_id: acknowledgement.database_id.get(),
            namespace_id: acknowledgement.namespace_id.get(),
            acknowledged_version: acknowledgement.acknowledged_version.get(),
        }
    }
}

/// Durable terminal evidence observed while replaying a publication stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublicationReplayTerminalRecord {
    pub key: CatalogPublicationReplayKey,
    pub outcome: CatalogPublicationReplayTerminalOutcome,
    pub durable_lsn: Option<u64>,
    pub durable_evidence_marker: Option<CatalogDurabilityMarker>,
    pub record_count: usize,
    pub audit_trace_id: String,
}

impl CatalogPublicationReplayTerminalRecord {
    pub fn committed_for_publication(publication: &CatalogPublicationReport) -> Self {
        Self {
            key: CatalogPublicationReplayKey::from_receipt(&publication.receipt),
            outcome: CatalogPublicationReplayTerminalOutcome::Committed,
            durable_lsn: publication.receipt.durable_lsn,
            durable_evidence_marker: publication.receipt.durable_evidence_marker,
            record_count: publication.receipt.record_count,
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
            return catalog_publication_error(
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
            return catalog_publication_error(
                "catalog publication replay terminal must advance by exactly one catalog version",
            );
        }
        if self.record_count == 0 {
            return catalog_publication_error(
                "catalog publication replay terminal record count must not be zero",
            );
        }
        if self.audit_trace_id.trim().is_empty() {
            return catalog_publication_error(
                "catalog publication replay terminal audit trace id must not be empty",
            );
        }
        if self.durable_lsn == Some(0) {
            return catalog_publication_error(
                "catalog publication replay terminal durable LSN must not be zero",
            );
        }
        if self.durable_lsn.is_none() && self.durable_evidence_marker.is_none() {
            return catalog_publication_error(
                "catalog publication replay terminal must carry durable LSN or marker evidence",
            );
        }
        Ok(())
    }

    pub fn validate_for_publication(
        &self,
        publication: &CatalogPublicationReport,
    ) -> AndromedaResult<()> {
        self.validate()?;
        if self.outcome != CatalogPublicationReplayTerminalOutcome::Committed {
            return catalog_publication_error(
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
            &publication.receipt.durable_lsn,
            "catalog publication replay terminal durable LSN must match publication",
        )?;
        require_equal(
            &self.durable_evidence_marker,
            &publication.receipt.durable_evidence_marker,
            "catalog publication replay terminal durable marker must match publication",
        )?;
        require_equal(
            &self.record_count,
            &publication.receipt.record_count,
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
pub enum CatalogPublicationSubscriptionReplayRecord {
    VisiblePublication {
        publication: CatalogPublicationReport,
        audit_evidence: CatalogVisibleChangeAuditEvidence,
    },
    SubscriptionAcknowledgement(CatalogSubscriptionAcknowledgement),
    Terminal(CatalogPublicationReplayTerminalRecord),
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
struct AppliedPublication {
    publication: CatalogPublicationReport,
    audit_evidence: CatalogVisibleChangeAuditEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PublishedVersionKey {
    database_id: u64,
    namespace_id: u64,
    catalog_version: u64,
}

impl PublishedVersionKey {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PublishedScopeKey {
    database_id: u64,
    namespace_id: u64,
}

impl PublishedScopeKey {
    fn from_publication(publication: &CatalogPublicationReport) -> Self {
        Self {
            database_id: publication.receipt.database_id.get(),
            namespace_id: publication.receipt.namespace_id.get(),
        }
    }
}

pub fn replay_publication_subscription_changes(
    records: impl IntoIterator<Item = CatalogPublicationSubscriptionReplayRecord>,
) -> AndromedaResult<CatalogPublicationSubscriptionReplaySummary> {
    let mut publications = BTreeMap::<CatalogPublicationReplayKey, AppliedPublication>::new();
    let mut publications_by_version =
        BTreeMap::<PublishedVersionKey, CatalogPublicationReplayKey>::new();
    let mut visible_versions = BTreeMap::<PublishedScopeKey, u64>::new();
    let mut acknowledgements =
        BTreeMap::<CatalogSubscriptionReplayKey, CatalogSubscriptionAcknowledgement>::new();
    let mut terminals =
        BTreeMap::<CatalogPublicationReplayKey, CatalogPublicationReplayTerminalRecord>::new();
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
                    return catalog_publication_error(
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
                    return catalog_publication_error(
                        "conflicting catalog publication replay record for the same publication identity",
                    );
                }

                let visible_key = PublishedVersionKey::from_publication(&applied.publication);
                if let Some(existing_key) = publications_by_version.get(&visible_key)
                    && existing_key != &key
                {
                    return catalog_publication_error(
                        "conflicting catalog publication replay record for the same visible catalog version",
                    );
                }

                let scope_key = PublishedScopeKey::from_publication(&applied.publication);
                if let Some(current_version) = visible_versions.get(&scope_key)
                    && applied.publication.receipt.previous_version.get() != *current_version
                {
                    return catalog_publication_error(
                        "catalog publication replay must preserve catalog version ordering",
                    );
                }

                publications_by_version.insert(visible_key, key);
                visible_versions.insert(scope_key, applied.publication.receipt.next_version.get());
                evidence.push(
                    CatalogPublicationSubscriptionReplayEvidence::PublicationApplied {
                        key,
                        audit_trace_id: applied.publication.audit_trace.trace_id.clone(),
                    },
                );
                publications.insert(key, applied);
                audit_before_visible_change_count += 1;
            }
            CatalogPublicationSubscriptionReplayRecord::SubscriptionAcknowledgement(
                acknowledgement,
            ) => {
                let version_key = PublishedVersionKey::from_acknowledgement(&acknowledgement);
                let Some(publication_key) = publications_by_version.get(&version_key).copied()
                else {
                    if let Some(publication) = publications.values().find(|publication| {
                        publication.publication.receipt.database_id == acknowledgement.database_id
                            && publication.publication.receipt.namespace_id
                                == acknowledgement.namespace_id
                    }) {
                        validate_subscription_acknowledgement_for_publication(
                            &acknowledgement,
                            &publication.publication,
                        )?;
                    }
                    return catalog_publication_error(
                        "catalog subscription acknowledgement replay requires a visible publication",
                    );
                };
                let Some(publication) = publications.get(&publication_key) else {
                    return catalog_publication_error(
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
                    return catalog_publication_error(
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
            }
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
                    return catalog_publication_error(
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
            }
        }
    }

    let final_visible_catalog_version = publications
        .values()
        .map(|publication| publication.publication.receipt.next_version)
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
