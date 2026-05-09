use andromeda_observe::TraceId;

use super::advisory_evidence::AdvisoryEvidenceSummary;
use super::decision::PlanDecisionReasonCode;
use super::{
    PLAN_CACHE_MAX_ENTRIES, PlanCacheKey, PlanCacheMissReason, PlanCandidateId,
    PlanDecisionEvidence, PlanDecisionOutcome, PlanSelectionOutcome,
};

/// Opaque entry stored by the bounded PlanCache gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanCacheEntry {
    key: PlanCacheKey,
    key_digest: [u8; 32],
    plan_id: PlanCandidateId,
    plan_digest: [u8; 32],
}

impl PlanCacheEntry {
    pub(crate) fn new(
        key: PlanCacheKey,
        plan_id: PlanCandidateId,
        plan_digest: [u8; 32],
    ) -> Result<Self, PlanCacheError> {
        if plan_digest == [0; 32] {
            return Err(PlanCacheError::PlanDigestZero);
        }
        Ok(Self {
            key,
            key_digest: key.digest(),
            plan_id,
            plan_digest,
        })
    }

    pub(crate) fn from_selection(selection: &PlanSelectionOutcome) -> Result<Self, PlanCacheError> {
        Self::new(
            selection.key(),
            selection.selected().plan_id(),
            selection.selected().plan_digest(),
        )
    }

    pub const fn key(self) -> PlanCacheKey {
        self.key
    }

    pub const fn key_digest(self) -> [u8; 32] {
        self.key_digest
    }

    pub const fn plan_id(self) -> PlanCandidateId {
        self.plan_id
    }

    pub const fn plan_digest(self) -> [u8; 32] {
        self.plan_digest
    }

    pub(crate) fn validates_for_key(&self, key: &PlanCacheKey) -> Result<(), PlanCacheError> {
        if self.key != *key {
            return Err(PlanCacheError::KeyMismatch);
        }
        if self.key_digest != key.digest() {
            return Err(PlanCacheError::KeyDigestMismatch);
        }
        if self.plan_digest == [0; 32] {
            return Err(PlanCacheError::PlanDigestZero);
        }
        Ok(())
    }
}

/// Bounded in-memory PlanCache gate.
///
/// The cache is deliberately small and deterministic.  It uses insertion
/// order eviction and exact [`PlanCacheKey`] equality only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedPlanCache {
    capacity: usize,
    entries: Vec<PlanCacheEntry>,
}

impl BoundedPlanCache {
    pub fn new(capacity: usize) -> Result<Self, PlanCacheError> {
        if capacity == 0 {
            return Err(PlanCacheError::CapacityZero);
        }
        if capacity > PLAN_CACHE_MAX_ENTRIES {
            return Err(PlanCacheError::CapacityTooLarge);
        }
        Ok(Self {
            capacity,
            entries: Vec::with_capacity(capacity),
        })
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert_selection(
        &mut self,
        selection: &PlanSelectionOutcome,
        trace_id: TraceId,
    ) -> Result<PlanCacheInsertReport, PlanCacheError> {
        self.insert_entry(PlanCacheEntry::from_selection(selection)?, trace_id)
    }

    pub(crate) fn insert_entry(
        &mut self,
        entry: PlanCacheEntry,
        trace_id: TraceId,
    ) -> Result<PlanCacheInsertReport, PlanCacheError> {
        if trace_id.is_zero() {
            return Err(PlanCacheError::TraceIdZero);
        }
        entry.validates_for_key(&entry.key)?;

        if let Some(position) = self
            .entries
            .iter()
            .position(|existing| existing.key == entry.key)
        {
            self.entries.remove(position);
        }

        let evicted = if self.entries.len() == self.capacity {
            Some(self.entries.remove(0))
        } else {
            None
        };
        self.entries.push(entry);

        let insert_trace = PlanDecisionEvidence::new(
            trace_id,
            entry.key,
            PlanDecisionOutcome::CacheInsert,
            PlanDecisionReasonCode::Inserted,
            Some(entry.plan_id),
            0,
            0,
            AdvisoryEvidenceSummary::empty(),
        );
        let eviction_trace = evicted.map(|evicted| {
            PlanDecisionEvidence::new(
                trace_id,
                evicted.key,
                PlanDecisionOutcome::CacheEvict,
                PlanDecisionReasonCode::CapacityEvictedOldest,
                Some(evicted.plan_id),
                0,
                0,
                AdvisoryEvidenceSummary::empty(),
            )
        });

        Ok(PlanCacheInsertReport {
            inserted: entry,
            evicted,
            insert_trace,
            eviction_trace,
        })
    }

    pub fn lookup(
        &self,
        key: PlanCacheKey,
        trace_id: TraceId,
    ) -> Result<PlanCacheLookupReport, PlanCacheError> {
        if trace_id.is_zero() {
            return Err(PlanCacheError::TraceIdZero);
        }
        let found = self
            .entries
            .iter()
            .copied()
            .find(|entry| entry.key == key)
            .and_then(|entry| match entry.validates_for_key(&key) {
                Ok(()) => Some(entry),
                Err(_) => None,
            });

        let (outcome, reason_code, selected_plan_id) = match found {
            Some(entry) => (
                PlanDecisionOutcome::CacheHit,
                PlanDecisionReasonCode::ExactKeyHit,
                Some(entry.plan_id),
            ),
            None => (
                PlanDecisionOutcome::CacheMiss,
                PlanDecisionReasonCode::ExactKeyMiss,
                None,
            ),
        };
        let cache_miss_reason = if found.is_some() {
            None
        } else {
            Some(self.classify_miss(&key))
        };
        let trace = PlanDecisionEvidence::new(
            trace_id,
            key,
            outcome,
            reason_code,
            selected_plan_id,
            0,
            0,
            AdvisoryEvidenceSummary::empty(),
        )
        .with_cache_miss_reason(cache_miss_reason);

        Ok(PlanCacheLookupReport {
            key,
            entry: found,
            trace,
        })
    }

    fn classify_miss(&self, key: &PlanCacheKey) -> PlanCacheMissReason {
        if self.entries.is_empty() {
            return PlanCacheMissReason::CacheEmpty;
        }

        let mut procedure_match = false;
        let mut contract_match = false;
        let mut catalog_match = false;
        let mut stats_match = false;
        let mut policy_match = false;
        let mut plan_class_match = false;

        for entry in &self.entries {
            if entry.key.procedure_id != key.procedure_id {
                continue;
            }
            procedure_match = true;
            if entry.key.contract_hash != key.contract_hash {
                continue;
            }
            contract_match = true;
            if entry.key.catalog_version != key.catalog_version {
                continue;
            }
            catalog_match = true;
            if entry.key.stats_version != key.stats_version {
                continue;
            }
            stats_match = true;
            if entry.key.policy_version != key.policy_version {
                continue;
            }
            policy_match = true;
            if entry.key.plan_class != key.plan_class {
                continue;
            }
            plan_class_match = true;
            if entry.key.shape_fingerprint != key.shape_fingerprint {
                continue;
            }
        }

        if !procedure_match {
            PlanCacheMissReason::NoProcedureEntry
        } else if !contract_match {
            PlanCacheMissReason::ContractHashMismatch
        } else if !catalog_match {
            PlanCacheMissReason::CatalogVersionMismatch
        } else if !stats_match {
            PlanCacheMissReason::StatsVersionMismatch
        } else if !policy_match {
            PlanCacheMissReason::PolicyVersionMismatch
        } else if !plan_class_match {
            PlanCacheMissReason::PlanClassMismatch
        } else {
            PlanCacheMissReason::ShapeFingerprintMismatch
        }
    }
}

/// Result of inserting into the bounded PlanCache gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCacheInsertReport {
    inserted: PlanCacheEntry,
    evicted: Option<PlanCacheEntry>,
    insert_trace: PlanDecisionEvidence,
    eviction_trace: Option<PlanDecisionEvidence>,
}

impl PlanCacheInsertReport {
    pub const fn inserted(&self) -> PlanCacheEntry {
        self.inserted
    }

    pub const fn evicted(&self) -> Option<PlanCacheEntry> {
        self.evicted
    }

    pub const fn insert_trace(&self) -> &PlanDecisionEvidence {
        &self.insert_trace
    }

    pub fn eviction_trace(&self) -> Option<&PlanDecisionEvidence> {
        self.eviction_trace.as_ref()
    }
}

/// Result of an exact-key cache lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCacheLookupReport {
    key: PlanCacheKey,
    entry: Option<PlanCacheEntry>,
    trace: PlanDecisionEvidence,
}

impl PlanCacheLookupReport {
    pub const fn key(&self) -> PlanCacheKey {
        self.key
    }

    pub const fn entry(&self) -> Option<PlanCacheEntry> {
        self.entry
    }

    pub const fn trace(&self) -> &PlanDecisionEvidence {
        &self.trace
    }

    pub const fn is_hit(&self) -> bool {
        self.entry.is_some()
    }
}

/// Errors raised by the bounded PlanCache gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCacheError {
    TraceIdZero,
    CapacityZero,
    CapacityTooLarge,
    KeyMismatch,
    KeyDigestMismatch,
    PlanDigestZero,
}

impl core::fmt::Display for PlanCacheError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanCacheError::TraceIdZero => {
                f.write_str("PlanCache trace-producing operations require a non-zero TraceId")
            }
            PlanCacheError::CapacityZero => f.write_str("PlanCache capacity must be non-zero"),
            PlanCacheError::CapacityTooLarge => {
                f.write_str("PlanCache capacity exceeds the bounded maximum")
            }
            PlanCacheError::KeyMismatch => {
                f.write_str("PlanCache entry key does not match lookup key")
            }
            PlanCacheError::KeyDigestMismatch => {
                f.write_str("PlanCache entry key digest does not match lookup key digest")
            }
            PlanCacheError::PlanDigestZero => {
                f.write_str("PlanCache entry plan digest must be non-zero")
            }
        }
    }
}
