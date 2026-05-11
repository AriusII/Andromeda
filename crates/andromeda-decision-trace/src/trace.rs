use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::{
    AdaptiveControl, DecisionTraceError, DecisionTraceSchemaVersion,
    cost_breakdown::{
        COST_BREAKDOWN_ALTERNATIVE_LIMIT, ColumnarPruningEvidence, PlanAlternativeEvidence,
    },
    version::DECISION_TRACE_SCHEMA_VERSION,
};

pub const DECISION_REASON_CODE_MAX_BYTES: usize = 64;
pub const DECISION_EXPLANATION_MAX_BYTES: usize = 512;
pub const EVIDENCE_LABEL_MAX_BYTES: usize = 64;
pub const EVIDENCE_REFERENCE_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionTraceId(u64);

impl DecisionTraceId {
    pub const fn new(value: u64) -> Result<Self, DecisionTraceError> {
        if value == 0 {
            Err(DecisionTraceError::TraceIdZero)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionFamily {
    OptimizerPlan,
    PlanCache,
    StatisticsPublication,
    StatisticsUse,
}

impl DecisionFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OptimizerPlan => "optimizer-plan",
            Self::PlanCache => "plan-cache",
            Self::StatisticsPublication => "statistics-publication",
            Self::StatisticsUse => "statistics-use",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionOutcome {
    Accepted,
    Rejected,
    Deferred,
    Disabled,
    Fallback,
    CacheHit,
    CacheMiss,
    CacheInsert,
    CacheEvict,
    Invalidate,
}

impl DecisionOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Deferred => "deferred",
            Self::Disabled => "disabled",
            Self::Fallback => "fallback",
            Self::CacheHit => "cache-hit",
            Self::CacheMiss => "cache-miss",
            Self::CacheInsert => "cache-insert",
            Self::CacheEvict => "cache-evict",
            Self::Invalidate => "invalidate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionReasonCode(String);

impl DecisionReasonCode {
    pub fn new(value: impl Into<String>) -> Result<Self, DecisionTraceError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DecisionTraceError::ReasonCodeEmpty);
        }
        if trimmed.len() > DECISION_REASON_CODE_MAX_BYTES {
            return Err(DecisionTraceError::ReasonCodeTooLong);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceLabel(String);

impl EvidenceLabel {
    pub fn new(value: impl Into<String>) -> Result<Self, DecisionTraceError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DecisionTraceError::EvidenceLabelEmpty);
        }
        if trimmed.len() > EVIDENCE_LABEL_MAX_BYTES {
            return Err(DecisionTraceError::EvidenceLabelTooLong);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvidenceDigest([u8; Self::LEN]);

impl EvidenceDigest {
    pub const LEN: usize = 32;

    pub fn new(bytes: [u8; Self::LEN]) -> Result<Self, DecisionTraceError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(DecisionTraceError::EvidenceDigestZero);
        }
        Ok(Self(bytes))
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvidence {
    label: EvidenceLabel,
    digest: EvidenceDigest,
}

impl TraceEvidence {
    pub fn new(label: EvidenceLabel, digest: EvidenceDigest) -> Self {
        Self { label, digest }
    }

    pub fn label(&self) -> &EvidenceLabel {
        &self.label
    }

    pub const fn digest(&self) -> EvidenceDigest {
        self.digest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VersionBinding {
    procedure_id: Option<ProcedureId>,
    contract_hash: Option<ContractHash>,
    catalog_version: Option<CatalogVersion>,
    stats_version: Option<StatsVersion>,
    policy_version: Option<PolicyVersion>,
}

impl VersionBinding {
    pub const fn empty() -> Self {
        Self {
            procedure_id: None,
            contract_hash: None,
            catalog_version: None,
            stats_version: None,
            policy_version: None,
        }
    }

    pub const fn for_procedure(binding: ProcedureContractBinding) -> Self {
        Self {
            procedure_id: Some(binding.procedure_id),
            contract_hash: Some(binding.contract_hash),
            catalog_version: Some(binding.catalog_version),
            stats_version: Some(binding.stats_version),
            policy_version: Some(binding.policy_version),
        }
    }

    pub const fn for_statistics(
        catalog_version: CatalogVersion,
        stats_version: StatsVersion,
        policy_version: Option<PolicyVersion>,
    ) -> Self {
        Self {
            procedure_id: None,
            contract_hash: None,
            catalog_version: Some(catalog_version),
            stats_version: Some(stats_version),
            policy_version,
        }
    }

    pub const fn with_policy_version(mut self, policy_version: PolicyVersion) -> Self {
        self.policy_version = Some(policy_version);
        self
    }

    pub const fn procedure_id(self) -> Option<ProcedureId> {
        self.procedure_id
    }

    pub const fn contract_hash(self) -> Option<ContractHash> {
        self.contract_hash
    }

    pub const fn catalog_version(self) -> Option<CatalogVersion> {
        self.catalog_version
    }

    pub const fn stats_version(self) -> Option<StatsVersion> {
        self.stats_version
    }

    pub const fn policy_version(self) -> Option<PolicyVersion> {
        self.policy_version
    }

    pub fn validate_complete_plan_binding(&self) -> Result<(), DecisionTraceError> {
        match self.procedure_id {
            Some(id) if id.get() != 0 => {},
            _ => return Err(DecisionTraceError::ProcedureIdZero),
        }
        match self.catalog_version {
            Some(version) if version.get() != 0 => {},
            _ => return Err(DecisionTraceError::CatalogVersionZero),
        }
        match self.contract_hash {
            Some(hash) if !hash.is_zero() => {},
            _ => return Err(DecisionTraceError::ContractHashZero),
        }
        match self.stats_version {
            Some(version) if !version.is_zero() => {},
            _ => return Err(DecisionTraceError::StatsVersionZero),
        }
        match self.policy_version {
            Some(version) if !version.is_zero() => {},
            _ => return Err(DecisionTraceError::PolicyVersionZero),
        }
        Ok(())
    }

    pub fn has_version_evidence(self) -> bool {
        self.catalog_version
            .is_some_and(|version| version.get() != 0)
            || self.stats_version.is_some_and(|version| !version.is_zero())
            || self
                .policy_version
                .is_some_and(|version| !version.is_zero())
            || self.contract_hash.is_some_and(|hash| !hash.is_zero())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionTrace {
    schema_version: DecisionTraceSchemaVersion,
    trace_id: DecisionTraceId,
    family: DecisionFamily,
    outcome: DecisionOutcome,
    reason_code: DecisionReasonCode,
    explanation: String,
    version_binding: VersionBinding,
    adaptive_control: Option<AdaptiveControl>,
    evidence: Vec<TraceEvidence>,
    alternative_costs: Vec<PlanAlternativeEvidence>,
    columnar_pruning: Option<ColumnarPruningEvidence>,
}

impl DecisionTrace {
    pub fn new(
        trace_id: DecisionTraceId,
        family: DecisionFamily,
        outcome: DecisionOutcome,
        reason_code: DecisionReasonCode,
        explanation: impl Into<String>,
        version_binding: VersionBinding,
    ) -> Result<Self, DecisionTraceError> {
        Self::new_with_control(
            trace_id,
            family,
            outcome,
            reason_code,
            explanation,
            version_binding,
            None,
        )
    }

    pub fn new_with_control(
        trace_id: DecisionTraceId,
        family: DecisionFamily,
        outcome: DecisionOutcome,
        reason_code: DecisionReasonCode,
        explanation: impl Into<String>,
        version_binding: VersionBinding,
        adaptive_control: Option<AdaptiveControl>,
    ) -> Result<Self, DecisionTraceError> {
        let explanation = normalize_explanation(explanation)?;
        Ok(Self {
            schema_version: DECISION_TRACE_SCHEMA_VERSION,
            trace_id,
            family,
            outcome,
            reason_code,
            explanation,
            version_binding,
            adaptive_control,
            evidence: Vec::new(),
            alternative_costs: Vec::new(),
            columnar_pruning: None,
        })
    }

    pub fn with_evidence(mut self, evidence: TraceEvidence) -> Result<Self, DecisionTraceError> {
        if self.evidence.len() == EVIDENCE_REFERENCE_LIMIT {
            return Err(DecisionTraceError::TooManyEvidenceReferences);
        }
        self.evidence.push(evidence);
        Ok(self)
    }

    pub const fn schema_version(&self) -> DecisionTraceSchemaVersion {
        self.schema_version
    }

    pub const fn trace_id(&self) -> DecisionTraceId {
        self.trace_id
    }

    pub const fn family(&self) -> DecisionFamily {
        self.family
    }

    pub const fn outcome(&self) -> DecisionOutcome {
        self.outcome
    }

    pub fn reason_code(&self) -> &DecisionReasonCode {
        &self.reason_code
    }

    pub fn explanation(&self) -> &str {
        self.explanation.as_str()
    }

    pub const fn version_binding(&self) -> VersionBinding {
        self.version_binding
    }

    pub const fn adaptive_control(&self) -> Option<AdaptiveControl> {
        self.adaptive_control
    }

    pub fn evidence(&self) -> &[TraceEvidence] {
        &self.evidence
    }

    pub fn is_observable(&self) -> bool {
        !self.explanation.trim().is_empty() && !self.reason_code.as_str().trim().is_empty()
    }

    pub fn is_versioned(&self) -> bool {
        self.version_binding.has_version_evidence()
    }

    pub fn is_disabled(&self) -> bool {
        self.outcome == DecisionOutcome::Disabled
            || self
                .adaptive_control
                .is_some_and(AdaptiveControl::is_disabled)
    }

    /// Attach a list of per-alternative cost breakdown entries to this trace.
    ///
    /// # Errors
    /// Returns [`DecisionTraceError::TooManyAlternatives`] when `alternatives`
    /// would push the total past [`COST_BREAKDOWN_ALTERNATIVE_LIMIT`].
    pub fn with_alternative_costs(
        mut self,
        alternatives: Vec<PlanAlternativeEvidence>,
    ) -> Result<Self, DecisionTraceError> {
        if alternatives.len() > COST_BREAKDOWN_ALTERNATIVE_LIMIT {
            return Err(DecisionTraceError::TooManyAlternatives);
        }
        self.alternative_costs = alternatives;
        Ok(self)
    }

    /// Per-alternative cost breakdown evidence attached to this trace.
    pub fn alternative_costs(&self) -> &[PlanAlternativeEvidence] {
        &self.alternative_costs
    }

    /// Attach advisory columnar pruning evidence to this trace.
    ///
    /// Overwrites any previously attached evidence.  Advisory only — the
    /// evidence records which chunks were skipped; it must never decide
    /// correctness.
    pub fn with_columnar_pruning(mut self, evidence: ColumnarPruningEvidence) -> Self {
        self.columnar_pruning = Some(evidence);
        self
    }

    /// Advisory columnar pruning evidence, if any was attached to this trace.
    pub fn columnar_pruning(&self) -> Option<ColumnarPruningEvidence> {
        self.columnar_pruning
    }
}

impl core::fmt::Display for DecisionTrace {
    /// Human-readable, deterministic representation of the trace.
    ///
    /// Cost alternatives are sorted by their `plan_kind` label so that output
    /// is stable regardless of insertion order.  Version fields are formatted
    /// as plain integers or hex prefixes; no `Debug` formatting is used.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // ── Header ──────────────────────────────────────────────────────────
        write!(
            f,
            "DecisionTrace[schema={} id={} family={} outcome={}]",
            self.schema_version.get(),
            self.trace_id.get(),
            self.family.as_str(),
            self.outcome.as_str(),
        )?;

        // ── Reason + explanation ────────────────────────────────────────────
        write!(f, "\n  reason: {}", self.reason_code.as_str())?;
        write!(f, "\n  explanation: {}", self.explanation)?;

        // ── Version binding ─────────────────────────────────────────────────
        let vb = &self.version_binding;
        if vb.has_version_evidence() {
            write!(f, "\n  versions:")?;
            if let Some(cv) = vb.catalog_version() {
                write!(f, " catalog={}", cv.get())?;
            }
            if let Some(sv) = vb.stats_version() {
                write!(f, " stats={}", sv.get())?;
            }
            if let Some(pv) = vb.policy_version() {
                write!(f, " policy=")?;
                for byte in pv.as_bytes().iter().take(4) {
                    write!(f, "{byte:02x}")?;
                }
            }
        }

        // ── Digest evidence ─────────────────────────────────────────────────
        for (i, ev) in self.evidence.iter().enumerate() {
            let digest = ev.digest().as_bytes();
            write!(f, "\n  evidence[{i}]: {} digest=", ev.label().as_str())?;
            for byte in digest.iter().take(8) {
                write!(f, "{byte:02x}")?;
            }
        }

        // ── Cost breakdown (sorted by plan_kind for determinism) ─────────────
        if !self.alternative_costs.is_empty() {
            let mut sorted: Vec<&PlanAlternativeEvidence> = self.alternative_costs.iter().collect();
            sorted.sort_by_key(|a| a.plan_kind());
            for (i, alt) in sorted.iter().enumerate() {
                let c = alt.cost();
                write!(
                    f,
                    "\n  cost_alternatives[{i}]: {} cpu={:.2} lio={:.2} pio={:.2} wal={:.2} tmp={:.2} net={:.2} risk={:.2} total={:.2}",
                    alt.plan_kind(),
                    c.cpu_cost,
                    c.logical_io_cost,
                    c.physical_io_cost,
                    c.wal_cost,
                    c.temp_cost,
                    c.network_cost,
                    c.risk_penalty_cost,
                    c.total_cost,
                )?;
                if alt.chosen() {
                    write!(f, " CHOSEN")?;
                } else if let Some(r) = alt.rejection() {
                    write!(f, " REJECTED({})", r.as_str())?;
                }
            }
        }

        // ── Columnar pruning advisory evidence ──────────────────────────────
        if let Some(cp) = self.columnar_pruning {
            write!(
                f,
                "\n  columnar-pruning: skipped={} total={} scannable={}",
                cp.skipped,
                cp.total,
                cp.scannable(),
            )?;
        }

        Ok(())
    }
}

fn normalize_explanation(value: impl Into<String>) -> Result<String, DecisionTraceError> {
    let value = value.into();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DecisionTraceError::ExplanationEmpty);
    }
    if trimmed.len() > DECISION_EXPLANATION_MAX_BYTES {
        return Err(DecisionTraceError::ExplanationTooLong);
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdaptiveFeature, DECISION_TRACE_SCHEMA_VERSION};

    fn policy(byte: u8) -> PolicyVersion {
        PolicyVersion::new([byte; PolicyVersion::LEN])
    }

    #[test]
    fn complete_plan_binding_rejects_zero_versions() {
        let binding = VersionBinding::for_procedure(ProcedureContractBinding {
            procedure_id: ProcedureId::new(7),
            catalog_version: CatalogVersion::new(8),
            contract_hash: ContractHash::test_vector(9),
            stats_version: StatsVersion::new(0),
            policy_version: policy(1),
        });

        assert_eq!(
            binding.validate_complete_plan_binding(),
            Err(DecisionTraceError::StatsVersionZero)
        );
    }

    #[test]
    fn trace_is_bounded_versioned_and_disableable() {
        let trace = DecisionTrace::new_with_control(
            DecisionTraceId::new(11).unwrap(),
            DecisionFamily::PlanCache,
            DecisionOutcome::Disabled,
            DecisionReasonCode::new("plan-cache-disabled").unwrap(),
            "plan cache reuse disabled by policy",
            VersionBinding::for_statistics(
                CatalogVersion::new(3),
                StatsVersion::new(4),
                Some(policy(2)),
            ),
            Some(AdaptiveControl::disabled(
                AdaptiveFeature::PlanCache,
                policy(2),
            )),
        )
        .unwrap();

        assert!(trace.is_observable());
        assert!(trace.is_versioned());
        assert!(trace.is_disabled());
        assert_eq!(trace.schema_version(), DECISION_TRACE_SCHEMA_VERSION);
    }

    #[test]
    fn evidence_count_is_capped() {
        let mut trace = DecisionTrace::new(
            DecisionTraceId::new(1).unwrap(),
            DecisionFamily::OptimizerPlan,
            DecisionOutcome::Accepted,
            DecisionReasonCode::new("accepted").unwrap(),
            "accepted bounded plan decision",
            VersionBinding::empty().with_policy_version(policy(1)),
        )
        .unwrap();

        for index in 0..EVIDENCE_REFERENCE_LIMIT {
            let label = EvidenceLabel::new(format!("evidence-{index}")).unwrap();
            let digest = EvidenceDigest::new([index as u8 + 1; 32]).unwrap();
            trace = trace
                .with_evidence(TraceEvidence::new(label, digest))
                .unwrap();
        }

        let err = trace
            .with_evidence(TraceEvidence::new(
                EvidenceLabel::new("overflow").unwrap(),
                EvidenceDigest::new([99; 32]).unwrap(),
            ))
            .unwrap_err();
        assert_eq!(err, DecisionTraceError::TooManyEvidenceReferences);
    }

    #[test]
    fn display_decision_trace_is_deterministic_and_includes_cost_breakdown() {
        use crate::cost_breakdown::{
            PlanAlternativeCost, PlanAlternativeEvidence, PlanRejectionReason,
        };

        // Build two alternatives: RangeScan (rejected) and PointLookup (chosen).
        // Insert in RangeScan-first order; the Display should sort alphabetically
        // so "point-lookup" appears before "range-scan".
        let range_cost = PlanAlternativeCost {
            cpu_cost: 50.0,
            logical_io_cost: 2.0,
            physical_io_cost: 8.0,
            wal_cost: 0.0,
            temp_cost: 2.0,
            network_cost: 0.0,
            risk_penalty_cost: 0.0,
            total_cost: 62.0,
        };
        let point_cost = PlanAlternativeCost {
            cpu_cost: 0.5,
            logical_io_cost: 2.0,
            physical_io_cost: 8.0,
            wal_cost: 0.0,
            temp_cost: 2.0,
            network_cost: 0.0,
            risk_penalty_cost: 0.0,
            total_cost: 12.5,
        };

        let alternatives = vec![
            PlanAlternativeEvidence::new(
                "range-scan",
                range_cost,
                false,
                Some(PlanRejectionReason::HigherCost),
            )
            .unwrap(),
            PlanAlternativeEvidence::new("point-lookup", point_cost, true, None).unwrap(),
        ];

        let trace = DecisionTrace::new(
            DecisionTraceId::new(42).unwrap(),
            DecisionFamily::OptimizerPlan,
            DecisionOutcome::Accepted,
            DecisionReasonCode::new("plan-chosen").unwrap(),
            "selected minimum-cost plan",
            VersionBinding::for_statistics(
                CatalogVersion::new(3),
                StatsVersion::new(4),
                Some(policy(5)),
            ),
        )
        .unwrap()
        .with_alternative_costs(alternatives)
        .unwrap();

        // Display must be deterministic: calling it twice yields identical output.
        let display_first = trace.to_string();
        let display_second = trace.to_string();
        assert_eq!(
            display_first, display_second,
            "Display must be deterministic"
        );

        // Header contains trace id, family, and outcome.
        assert!(
            display_first.contains("id=42"),
            "missing trace id: {display_first}"
        );
        assert!(
            display_first.contains("family=optimizer-plan"),
            "missing family: {display_first}"
        );
        assert!(
            display_first.contains("outcome=accepted"),
            "missing outcome: {display_first}"
        );

        // Explanation and reason are present.
        assert!(
            display_first.contains("reason: plan-chosen"),
            "missing reason: {display_first}"
        );
        assert!(
            display_first.contains("explanation: selected minimum-cost plan"),
            "missing explanation: {display_first}"
        );

        // Version binding is present.
        assert!(
            display_first.contains("catalog=3"),
            "missing catalog version: {display_first}"
        );
        assert!(
            display_first.contains("stats=4"),
            "missing stats version: {display_first}"
        );

        // Cost breakdown appears sorted: point-lookup before range-scan.
        let pos_point = display_first
            .find("point-lookup")
            .expect("missing point-lookup in display");
        let pos_range = display_first
            .find("range-scan")
            .expect("missing range-scan in display");
        assert!(
            pos_point < pos_range,
            "point-lookup must appear before range-scan (sorted by plan_kind)"
        );

        // Chosen marker for point-lookup.
        assert!(
            display_first.contains("CHOSEN"),
            "missing CHOSEN marker: {display_first}"
        );

        // Rejection reason for range-scan.
        assert!(
            display_first.contains("REJECTED(higher-cost)"),
            "missing REJECTED label: {display_first}"
        );

        // Total costs are formatted as two-decimal floats.
        assert!(
            display_first.contains("total=12.50"),
            "missing point-lookup total: {display_first}"
        );
        assert!(
            display_first.contains("total=62.00"),
            "missing range-scan total: {display_first}"
        );
    }

    #[test]
    fn alternative_costs_limit_is_enforced() {
        use crate::cost_breakdown::{COST_BREAKDOWN_ALTERNATIVE_LIMIT, PlanAlternativeEvidence};

        let trace = DecisionTrace::new(
            DecisionTraceId::new(1).unwrap(),
            DecisionFamily::OptimizerPlan,
            DecisionOutcome::Accepted,
            DecisionReasonCode::new("plan-chosen").unwrap(),
            "accepted plan",
            VersionBinding::empty().with_policy_version(policy(1)),
        )
        .unwrap();

        // Build one more than the limit.
        let alts: Vec<PlanAlternativeEvidence> = (0..=COST_BREAKDOWN_ALTERNATIVE_LIMIT)
            .map(|i| {
                PlanAlternativeEvidence::new(
                    format!("plan-kind-{i:02}"),
                    crate::cost_breakdown::PlanAlternativeCost::zero(),
                    i == 0,
                    None,
                )
                .unwrap()
            })
            .collect();

        let err = trace.with_alternative_costs(alts).unwrap_err();
        assert_eq!(err, DecisionTraceError::TooManyAlternatives);
    }
}
