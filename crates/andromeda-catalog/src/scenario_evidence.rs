//! V0 scored, expirable, *non-authoritative* scenario evidence scaffold.
//!
//! **Status: SCAFFOLD ONLY.**  This module does not run benchmarks, does
//! not collect timings, does not maintain a workload catalog, and does
//! **not** make optimizer decisions.  It only models the bounded shape of
//! a [`ScenarioEvidence`] record that a future Procedure-only optimizer
//! may consume *as advisory input*.  The optimizer remains the sole
//! authoritative decision maker.
//!
//! ## Doctrine pinned by this module
//!
//! 1. **Advisory by construction.**  Every [`ScenarioEvidence`] reports
//!    `is_authoritative() == false`.  There is no constructor, no setter,
//!    and no variant that can flip the flag.  Optimizer code that ever
//!    needs to know "may I treat this as binding?" gets a single,
//!    unambiguous, type-level `false`.
//! 2. **Bounded score and confidence.**  Scores and confidence values
//!    live on a fixed `0..=1000` integer scale ("permille").  No floats,
//!    no implicit clamping.  Out-of-range values are rejected at
//!    construction.  This guarantees deterministic byte-for-byte digests
//!    on every node.
//! 3. **Mandatory validity window.**  Every record carries
//!    `issued_at < expires_at` ([`EngineTimestamp`] millis).  Helpers
//!    such as [`ScenarioEvidence::is_expired_at`] and
//!    [`ScenarioEvidence::validate_for_use_at`] make expiry an explicit
//!    consumer-side check.  There is **no silent expiry bypass**: the
//!    consumer must pass a clock reading, and an expired record raises a
//!    closed error variant.
//! 4. **Explicit version targeting.**  The target couples a
//!    `ProcedureId` with a `StatsVersion` (and optional `ContractHash` /
//!    [`PlanClass`]).  Bumping the stats version or contract hash
//!    automatically separates digests so a benchmark gathered against
//!    one snapshot can never silently apply to a different snapshot.
//! 5. **Deterministic digest.**  [`ScenarioEvidence::digest`] is a
//!    SHA-256 over a domain-tagged, byte-tagged, little-endian encoding
//!    of every field.  Two records that compare equal must produce the
//!    same digest on every node, regardless of build target.
//! 6. **Closed validation errors.**  [`ScenarioEvidenceError`] enumerates
//!    every reject reason.  No free-form strings, no SQL surface, no
//!    unbounded payloads.
//!
//! ## What this module deliberately does NOT do
//!
//! - It does not execute a workload or measure runtime.
//! - It does not select a plan, score a plan, or rank procedures.
//! - It does not own a publication store, eviction policy, or wire
//!   format.
//! - It does not interpret SQL.  The project remains Procedure-only.
//! - It does not let evidence become authoritative.  Even a perfectly
//!   scored, freshly-issued record is advisory input to the optimizer.

use andromeda_core::{ContractHash, EngineTimestamp, ProcedureId};

use crate::contracts::StatsVersion;
use crate::digest::Sha256;
use crate::plan_cache::PlanClass;

/// Domain tag absorbed at the start of every scenario-evidence digest.
const SCENARIO_EVIDENCE_DOMAIN: &[u8] = b"andromeda.scenario_evidence.v0";

/// Bounded scenario-kind taxonomy.
///
/// The variants are closed.  Adding a variant is a doctrine change
/// because it expands what kind of advisory signal the optimizer is
/// allowed to react to.  Each variant carries a stable `as_tag` byte
/// that participates in the digest so two scenarios of different kinds
/// never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScenarioKind {
    /// A bounded micro-benchmark targeting a single procedure under a
    /// fixed parameter shape.
    Microbenchmark,
    /// A representative procedure-level workload run, multiple
    /// invocations of one procedure.
    ProcedureWorkload,
    /// A regression probe used to detect performance drift between
    /// known-good baselines.
    RegressionProbe,
    /// A synthetic load designed to exercise a particular plan class.
    SyntheticLoad,
}

impl ScenarioKind {
    /// Stable tag byte folded into the evidence digest.  Reordering or
    /// reusing tag bytes is a doctrine change.
    pub const fn as_tag(self) -> u8 {
        match self {
            ScenarioKind::Microbenchmark => 0x01,
            ScenarioKind::ProcedureWorkload => 0x02,
            ScenarioKind::RegressionProbe => 0x03,
            ScenarioKind::SyntheticLoad => 0x04,
        }
    }

    /// Total number of variants.  Asserted by tests so accidental growth
    /// is flagged immediately.
    pub const VARIANT_COUNT: usize = 4;
}

/// Stable scenario identity.  Non-zero so a "no scenario" sentinel
/// cannot accidentally produce a valid digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScenarioId(u64);

impl ScenarioId {
    /// Construct a `ScenarioId`.  Returns `None` for the reserved zero
    /// value.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Bounded score on a fixed `0..=1000` integer scale ("permille").
///
/// The interpretation is intentionally left abstract at this layer:
/// callers decide whether higher means "faster", "preferred", or
/// "cheaper".  The bounded integer representation guarantees
/// byte-identical digests on every node and forbids float drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceScore(u16);

impl EvidenceScore {
    /// Maximum admissible raw value.  `1000` corresponds to "1.000".
    pub const MAX_RAW: u16 = 1_000;

    /// The zero score.  Always valid.
    pub const ZERO: Self = Self(0);

    /// Construct from a raw permille value.  Rejects anything strictly
    /// greater than [`Self::MAX_RAW`].  No clamping: out-of-range
    /// callers are bugs and must be reported as such.
    pub const fn from_permille(value: u16) -> Result<Self, ScenarioEvidenceError> {
        if value > Self::MAX_RAW {
            Err(ScenarioEvidenceError::ScoreOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

/// Bounded confidence on a fixed `0..=1000` integer scale ("permille").
///
/// Independent of [`EvidenceScore`] so a high-magnitude signal with low
/// statistical confidence is representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EvidenceConfidence(u16);

impl EvidenceConfidence {
    pub const MAX_RAW: u16 = 1_000;

    pub const ZERO: Self = Self(0);

    pub const fn from_permille(value: u16) -> Result<Self, ScenarioEvidenceError> {
        if value > Self::MAX_RAW {
            Err(ScenarioEvidenceError::ConfidenceOutOfRange)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn permille(self) -> u16 {
        self.0
    }
}

/// Half-open validity window `[issued_at, expires_at)`.
///
/// `issued_at` must be strictly before `expires_at`; `expires_at == 0`
/// is rejected so the "absent expiry" sentinel cannot accidentally make
/// every record valid forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValidityWindow {
    issued_at: EngineTimestamp,
    expires_at: EngineTimestamp,
}

impl ValidityWindow {
    /// Construct a window.  Validates `issued_at < expires_at` and
    /// `expires_at != 0`.
    pub fn new(
        issued_at: EngineTimestamp,
        expires_at: EngineTimestamp,
    ) -> Result<Self, ScenarioEvidenceError> {
        if expires_at.is_zero() {
            return Err(ScenarioEvidenceError::ExpiryMustBeNonZero);
        }
        if issued_at.as_unix_millis() >= expires_at.as_unix_millis() {
            return Err(ScenarioEvidenceError::IssuedNotBeforeExpiry);
        }
        Ok(Self {
            issued_at,
            expires_at,
        })
    }

    pub const fn issued_at(self) -> EngineTimestamp {
        self.issued_at
    }

    pub const fn expires_at(self) -> EngineTimestamp {
        self.expires_at
    }

    /// `true` when `now >= expires_at`.  No grace period; consumers
    /// that want one must apply it explicitly *before* calling.
    pub fn is_expired_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() >= self.expires_at.as_unix_millis()
    }

    /// `true` when `now < issued_at`.  Records issued in the future are
    /// not yet usable; surfacing this as a distinct condition prevents a
    /// silent "treat as valid" path.
    pub fn is_not_yet_valid_at(self, now: EngineTimestamp) -> bool {
        now.as_unix_millis() < self.issued_at.as_unix_millis()
    }
}

/// Bounded targeting metadata.
///
/// Couples a procedure id with the statistics snapshot the evidence was
/// gathered against, plus optional plan-class and contract-hash refinements.
/// Optionality is encoded with explicit `Option` so the digest distinguishes
/// "absent" from "present-but-zero".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioTarget {
    pub procedure_id: ProcedureId,
    pub stats_version: StatsVersion,
    pub plan_class: Option<PlanClass>,
    pub contract_hash: Option<ContractHash>,
}

impl ScenarioTarget {
    /// Validate the target.  `procedure_id` and `stats_version` must be
    /// non-zero so evidence cannot be implicitly addressed at a
    /// "no procedure" / "no stats" sentinel.
    pub fn validate(&self) -> Result<(), ScenarioEvidenceError> {
        if self.procedure_id.get() == 0 {
            return Err(ScenarioEvidenceError::TargetProcedureIdZero);
        }
        if self.stats_version.get() == 0 {
            return Err(ScenarioEvidenceError::TargetStatsVersionZero);
        }
        if let Some(hash) = self.contract_hash
            && hash.is_zero()
        {
            return Err(ScenarioEvidenceError::TargetContractHashZero);
        }
        Ok(())
    }

    // NOTE: Target field absorption is performed inline by
    // [`ScenarioEvidence::digest`] so that field ordering and tag bytes
    // remain visible alongside the rest of the evidence layout.  This
    // avoids accidentally splitting the digest layout across files.
}

/// V0 scored, expirable, advisory scenario evidence record.
///
/// Construct via [`ScenarioEvidence::new`], which performs all bounded
/// validation in a single place.  All fields are private so external
/// callers cannot bypass validation by direct struct literal
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScenarioEvidence {
    scenario_id: ScenarioId,
    kind: ScenarioKind,
    target: ScenarioTarget,
    score: EvidenceScore,
    confidence: EvidenceConfidence,
    validity: ValidityWindow,
}

impl ScenarioEvidence {
    /// Construct an evidence record after running every bounded check.
    ///
    /// On success, the resulting record is guaranteed to:
    /// - have a non-zero scenario id, procedure id, and stats version,
    /// - carry a score and confidence in `0..=1000`,
    /// - have a validity window with `issued_at < expires_at` and
    ///   `expires_at > 0`,
    /// - carry a non-zero `contract_hash` whenever one is supplied.
    pub fn new(
        scenario_id: ScenarioId,
        kind: ScenarioKind,
        target: ScenarioTarget,
        score: EvidenceScore,
        confidence: EvidenceConfidence,
        validity: ValidityWindow,
    ) -> Result<Self, ScenarioEvidenceError> {
        target.validate()?;
        Ok(Self {
            scenario_id,
            kind,
            target,
            score,
            confidence,
            validity,
        })
    }

    pub const fn scenario_id(&self) -> ScenarioId {
        self.scenario_id
    }

    pub const fn kind(&self) -> ScenarioKind {
        self.kind
    }

    pub const fn target(&self) -> &ScenarioTarget {
        &self.target
    }

    pub const fn score(&self) -> EvidenceScore {
        self.score
    }

    pub const fn confidence(&self) -> EvidenceConfidence {
        self.confidence
    }

    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }

    /// **Doctrine invariant: scenario evidence is never authoritative.**
    /// This always returns `false`.  It exists so optimizer-side code
    /// can express the doctrine at the type level (`assert!(!ev.is_authoritative())`)
    /// without conditional logic.
    pub const fn is_authoritative(&self) -> bool {
        false
    }

    /// `true` when `now >= expires_at`.
    pub fn is_expired_at(&self, now: EngineTimestamp) -> bool {
        self.validity.is_expired_at(now)
    }

    /// Validate the record for consumption *at* `now`.  Returns:
    /// - `Err(NotYetValid)` if the record is issued in the future,
    /// - `Err(Expired)` if the record has expired,
    /// - `Ok(())` otherwise.
    ///
    /// Optimizer code is required to call this (or
    /// [`Self::is_expired_at`]) before consuming evidence.  Calling
    /// neither is a doctrine violation: there is no silent bypass.
    pub fn validate_for_use_at(&self, now: EngineTimestamp) -> Result<(), ScenarioEvidenceError> {
        if self.validity.is_not_yet_valid_at(now) {
            return Err(ScenarioEvidenceError::NotYetValid);
        }
        if self.validity.is_expired_at(now) {
            return Err(ScenarioEvidenceError::Expired);
        }
        Ok(())
    }

    /// Compute a deterministic 32-byte digest over every field.
    ///
    /// Equal records produce equal digests; differing records produce
    /// differing digests with overwhelming probability (SHA-256).  Tag
    /// bytes and little-endian widths make the encoding stable across
    /// architectures.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(SCENARIO_EVIDENCE_DOMAIN);

        hasher.update(&[0xE0]);
        hasher.update(&self.scenario_id.get().to_le_bytes());

        hasher.update(&[0xE1, self.kind.as_tag()]);

        hasher.update(&[0xE2]);
        hasher.update(&self.target.procedure_id.get().to_le_bytes());
        hasher.update(&[0xE3]);
        hasher.update(&self.target.stats_version.get().to_le_bytes());
        hasher.update(&[0xE4]);
        match self.target.plan_class {
            Some(pc) => hasher.update(&[0x01, pc.as_tag()]),
            None => hasher.update(&[0x00, 0x00]),
        }
        hasher.update(&[0xE5]);
        match self.target.contract_hash {
            Some(hash) => {
                hasher.update(&[0x01]);
                hasher.update(&hash.as_bytes());
            }
            None => {
                hasher.update(&[0x00]);
                hasher.update(&[0u8; ContractHash::LEN]);
            }
        }

        hasher.update(&[0xE6]);
        hasher.update(&self.score.permille().to_le_bytes());
        hasher.update(&[0xE7]);
        hasher.update(&self.confidence.permille().to_le_bytes());

        hasher.update(&[0xE8]);
        hasher.update(&self.validity.issued_at.as_unix_millis().to_le_bytes());
        hasher.update(&[0xE9]);
        hasher.update(&self.validity.expires_at.as_unix_millis().to_le_bytes());

        // Authoritative flag is folded in as a constant `0` so a future
        // attempt to add an "authoritative" variant would change the
        // digest layout and break compatibility loudly.
        hasher.update(&[0xEA, 0x00]);

        hasher.finalize()
    }
}

/// Closed set of reasons a scenario-evidence record may be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioEvidenceError {
    /// Score raw value exceeded [`EvidenceScore::MAX_RAW`].
    ScoreOutOfRange,
    /// Confidence raw value exceeded [`EvidenceConfidence::MAX_RAW`].
    ConfidenceOutOfRange,
    /// Validity window had `expires_at == 0`.
    ExpiryMustBeNonZero,
    /// Validity window did not satisfy `issued_at < expires_at`.
    IssuedNotBeforeExpiry,
    /// Target carried a zero `ProcedureId`.
    TargetProcedureIdZero,
    /// Target carried a zero `StatsVersion`.
    TargetStatsVersionZero,
    /// Target carried an explicit but all-zero `ContractHash`.
    TargetContractHashZero,
    /// `validate_for_use_at` was called with `now < issued_at`.
    NotYetValid,
    /// `validate_for_use_at` was called with `now >= expires_at`.
    Expired,
}

impl core::fmt::Display for ScenarioEvidenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScenarioEvidenceError::ScoreOutOfRange => {
                f.write_str("EvidenceScore raw value must be in 0..=1000")
            }
            ScenarioEvidenceError::ConfidenceOutOfRange => {
                f.write_str("EvidenceConfidence raw value must be in 0..=1000")
            }
            ScenarioEvidenceError::ExpiryMustBeNonZero => {
                f.write_str("ValidityWindow.expires_at must be non-zero")
            }
            ScenarioEvidenceError::IssuedNotBeforeExpiry => {
                f.write_str("ValidityWindow requires issued_at < expires_at")
            }
            ScenarioEvidenceError::TargetProcedureIdZero => {
                f.write_str("ScenarioTarget.procedure_id must be non-zero")
            }
            ScenarioEvidenceError::TargetStatsVersionZero => {
                f.write_str("ScenarioTarget.stats_version must be non-zero")
            }
            ScenarioEvidenceError::TargetContractHashZero => {
                f.write_str("ScenarioTarget.contract_hash, when present, must be non-zero")
            }
            ScenarioEvidenceError::NotYetValid => {
                f.write_str("ScenarioEvidence is not yet valid at the supplied clock reading")
            }
            ScenarioEvidenceError::Expired => {
                f.write_str("ScenarioEvidence has expired at the supplied clock reading")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(ms: u64) -> EngineTimestamp {
        EngineTimestamp::from_unix_millis(ms)
    }

    fn target() -> ScenarioTarget {
        ScenarioTarget {
            procedure_id: ProcedureId::new(7),
            stats_version: StatsVersion::new(3),
            plan_class: Some(PlanClass::ParameterShape),
            contract_hash: Some(ContractHash::new([0xAB; ContractHash::LEN])),
        }
    }

    fn evidence_at(
        score: u16,
        confidence: u16,
        issued: u64,
        expires: u64,
    ) -> Result<ScenarioEvidence, ScenarioEvidenceError> {
        ScenarioEvidence::new(
            ScenarioId::new(42).expect("non-zero"),
            ScenarioKind::Microbenchmark,
            target(),
            EvidenceScore::from_permille(score)?,
            EvidenceConfidence::from_permille(confidence)?,
            ValidityWindow::new(ts(issued), ts(expires))?,
        )
    }

    #[test]
    fn scenario_kind_variant_count_is_bounded() {
        assert_eq!(ScenarioKind::VARIANT_COUNT, 4);
        let tags = [
            ScenarioKind::Microbenchmark.as_tag(),
            ScenarioKind::ProcedureWorkload.as_tag(),
            ScenarioKind::RegressionProbe.as_tag(),
            ScenarioKind::SyntheticLoad.as_tag(),
        ];
        let mut sorted = tags;
        sorted.sort_unstable();
        assert!(
            sorted.windows(2).all(|pair| pair[0] != pair[1]),
            "ScenarioKind tags must be unique"
        );
    }

    #[test]
    fn scenario_id_rejects_zero() {
        assert!(ScenarioId::new(0).is_none());
        assert_eq!(ScenarioId::new(5).unwrap().get(), 5);
    }

    #[test]
    fn score_and_confidence_enforce_bounds() {
        assert!(EvidenceScore::from_permille(0).is_ok());
        assert!(EvidenceScore::from_permille(1_000).is_ok());
        assert_eq!(
            EvidenceScore::from_permille(1_001),
            Err(ScenarioEvidenceError::ScoreOutOfRange)
        );

        assert!(EvidenceConfidence::from_permille(0).is_ok());
        assert!(EvidenceConfidence::from_permille(1_000).is_ok());
        assert_eq!(
            EvidenceConfidence::from_permille(1_001),
            Err(ScenarioEvidenceError::ConfidenceOutOfRange)
        );
    }

    #[test]
    fn validity_window_requires_issued_before_expiry_and_nonzero_expiry() {
        assert_eq!(
            ValidityWindow::new(ts(10), ts(0)),
            Err(ScenarioEvidenceError::ExpiryMustBeNonZero)
        );
        assert_eq!(
            ValidityWindow::new(ts(10), ts(10)),
            Err(ScenarioEvidenceError::IssuedNotBeforeExpiry)
        );
        assert_eq!(
            ValidityWindow::new(ts(20), ts(10)),
            Err(ScenarioEvidenceError::IssuedNotBeforeExpiry)
        );
        assert!(ValidityWindow::new(ts(10), ts(20)).is_ok());
    }

    #[test]
    fn target_validation_rejects_zero_identity_fields() {
        let mut t = target();
        t.procedure_id = ProcedureId::new(0);
        assert_eq!(
            ScenarioEvidence::new(
                ScenarioId::new(1).unwrap(),
                ScenarioKind::Microbenchmark,
                t,
                EvidenceScore::ZERO,
                EvidenceConfidence::ZERO,
                ValidityWindow::new(ts(1), ts(2)).unwrap(),
            ),
            Err(ScenarioEvidenceError::TargetProcedureIdZero)
        );

        let mut t = target();
        t.stats_version = StatsVersion::new(0);
        assert_eq!(
            ScenarioEvidence::new(
                ScenarioId::new(1).unwrap(),
                ScenarioKind::Microbenchmark,
                t,
                EvidenceScore::ZERO,
                EvidenceConfidence::ZERO,
                ValidityWindow::new(ts(1), ts(2)).unwrap(),
            ),
            Err(ScenarioEvidenceError::TargetStatsVersionZero)
        );

        let mut t = target();
        t.contract_hash = Some(ContractHash::new([0u8; ContractHash::LEN]));
        assert_eq!(
            ScenarioEvidence::new(
                ScenarioId::new(1).unwrap(),
                ScenarioKind::Microbenchmark,
                t,
                EvidenceScore::ZERO,
                EvidenceConfidence::ZERO,
                ValidityWindow::new(ts(1), ts(2)).unwrap(),
            ),
            Err(ScenarioEvidenceError::TargetContractHashZero)
        );
    }

    #[test]
    fn evidence_is_always_advisory() {
        // Doctrine: even at peak score and confidence, the record is
        // never authoritative.  This is a *type-level* invariant, not a
        // runtime flag the caller could flip.
        let ev = evidence_at(1_000, 1_000, 100, 200).unwrap();
        assert!(!ev.is_authoritative());
    }

    #[test]
    fn validate_for_use_at_rejects_expired_and_not_yet_valid() {
        let ev = evidence_at(500, 800, 100, 200).unwrap();

        // Before issuance.
        assert_eq!(
            ev.validate_for_use_at(ts(50)),
            Err(ScenarioEvidenceError::NotYetValid)
        );
        // At issuance: valid.
        assert!(ev.validate_for_use_at(ts(100)).is_ok());
        // Inside window.
        assert!(ev.validate_for_use_at(ts(150)).is_ok());
        // At expiry: expired (half-open).
        assert_eq!(
            ev.validate_for_use_at(ts(200)),
            Err(ScenarioEvidenceError::Expired)
        );
        // After expiry.
        assert_eq!(
            ev.validate_for_use_at(ts(999)),
            Err(ScenarioEvidenceError::Expired)
        );

        assert!(!ev.is_expired_at(ts(199)));
        assert!(ev.is_expired_at(ts(200)));
    }

    #[test]
    fn digest_is_deterministic_for_equal_records() {
        let a = evidence_at(500, 800, 100, 200).unwrap();
        let b = evidence_at(500, 800, 100, 200).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn digest_separates_by_score_confidence_and_window() {
        let base = evidence_at(500, 800, 100, 200).unwrap();
        let other_score = evidence_at(501, 800, 100, 200).unwrap();
        let other_conf = evidence_at(500, 801, 100, 200).unwrap();
        let other_issued = evidence_at(500, 800, 101, 200).unwrap();
        let other_expiry = evidence_at(500, 800, 100, 201).unwrap();

        let d = base.digest();
        assert_ne!(d, other_score.digest());
        assert_ne!(d, other_conf.digest());
        assert_ne!(d, other_issued.digest());
        assert_ne!(d, other_expiry.digest());
    }

    #[test]
    fn digest_separates_by_target_version_and_class() {
        let base = evidence_at(500, 800, 100, 200).unwrap();

        let mut other_stats_target = target();
        other_stats_target.stats_version = StatsVersion::new(4);
        let other_stats = ScenarioEvidence::new(
            ScenarioId::new(42).unwrap(),
            ScenarioKind::Microbenchmark,
            other_stats_target,
            EvidenceScore::from_permille(500).unwrap(),
            EvidenceConfidence::from_permille(800).unwrap(),
            ValidityWindow::new(ts(100), ts(200)).unwrap(),
        )
        .unwrap();

        let mut other_class_target = target();
        other_class_target.plan_class = Some(PlanClass::Cardinality);
        let other_class = ScenarioEvidence::new(
            ScenarioId::new(42).unwrap(),
            ScenarioKind::Microbenchmark,
            other_class_target,
            EvidenceScore::from_permille(500).unwrap(),
            EvidenceConfidence::from_permille(800).unwrap(),
            ValidityWindow::new(ts(100), ts(200)).unwrap(),
        )
        .unwrap();

        let mut absent_class_target = target();
        absent_class_target.plan_class = None;
        let absent_class = ScenarioEvidence::new(
            ScenarioId::new(42).unwrap(),
            ScenarioKind::Microbenchmark,
            absent_class_target,
            EvidenceScore::from_permille(500).unwrap(),
            EvidenceConfidence::from_permille(800).unwrap(),
            ValidityWindow::new(ts(100), ts(200)).unwrap(),
        )
        .unwrap();

        let mut absent_hash_target = target();
        absent_hash_target.contract_hash = None;
        let absent_hash = ScenarioEvidence::new(
            ScenarioId::new(42).unwrap(),
            ScenarioKind::Microbenchmark,
            absent_hash_target,
            EvidenceScore::from_permille(500).unwrap(),
            EvidenceConfidence::from_permille(800).unwrap(),
            ValidityWindow::new(ts(100), ts(200)).unwrap(),
        )
        .unwrap();

        let d = base.digest();
        assert_ne!(d, other_stats.digest());
        assert_ne!(d, other_class.digest());
        // Absent vs Some(...) must differ from Some(other) and from each other.
        assert_ne!(d, absent_class.digest());
        assert_ne!(other_class.digest(), absent_class.digest());
        assert_ne!(d, absent_hash.digest());
    }

    #[test]
    fn digest_separates_by_scenario_kind_and_id() {
        let base = evidence_at(500, 800, 100, 200).unwrap();

        let other_kind = ScenarioEvidence::new(
            ScenarioId::new(42).unwrap(),
            ScenarioKind::RegressionProbe,
            target(),
            EvidenceScore::from_permille(500).unwrap(),
            EvidenceConfidence::from_permille(800).unwrap(),
            ValidityWindow::new(ts(100), ts(200)).unwrap(),
        )
        .unwrap();

        let other_id = ScenarioEvidence::new(
            ScenarioId::new(43).unwrap(),
            ScenarioKind::Microbenchmark,
            target(),
            EvidenceScore::from_permille(500).unwrap(),
            EvidenceConfidence::from_permille(800).unwrap(),
            ValidityWindow::new(ts(100), ts(200)).unwrap(),
        )
        .unwrap();

        assert_ne!(base.digest(), other_kind.digest());
        assert_ne!(base.digest(), other_id.digest());
    }
}
