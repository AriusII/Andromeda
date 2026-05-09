use andromeda_digest::Sha256;
use andromeda_types::ContractHash;

use super::ScenarioEvidence;

/// Domain tag absorbed at the start of every scenario-evidence digest.
const SCENARIO_EVIDENCE_DOMAIN: &[u8] = b"andromeda.scenario_evidence.v0";

/// Compute a deterministic 32-byte digest over every field.
pub(super) fn compute_digest(evidence: &ScenarioEvidence) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(SCENARIO_EVIDENCE_DOMAIN);

    hasher.update(&[0xE0]);
    hasher.update(&evidence.scenario_id().get().to_le_bytes());

    hasher.update(&[0xE1, evidence.kind().as_tag()]);

    let target = evidence.target();
    hasher.update(&[0xE2]);
    hasher.update(&target.procedure_id.get().to_le_bytes());
    hasher.update(&[0xE3]);
    hasher.update(&target.catalog_version.get().to_le_bytes());
    hasher.update(&[0xE4]);
    hasher.update(&target.stats_version.get().to_le_bytes());
    hasher.update(&[0xE5]);
    match target.plan_class {
        Some(pc) => hasher.update(&[0x01, pc.as_tag()]),
        None => hasher.update(&[0x00, 0x00]),
    }
    hasher.update(&[0xE6]);
    match target.contract_hash {
        Some(hash) => {
            hasher.update(&[0x01]);
            hasher.update(&hash.as_bytes());
        },
        None => {
            hasher.update(&[0x00]);
            hasher.update(&[0u8; ContractHash::LEN]);
        },
    }

    hasher.update(&[0xE7]);
    hasher.update(&evidence.score().permille().to_le_bytes());
    hasher.update(&[0xE8]);
    hasher.update(&evidence.confidence().permille().to_le_bytes());

    let validity = evidence.validity();
    hasher.update(&[0xE9]);
    hasher.update(&validity.issued_at().as_unix_millis().to_le_bytes());
    hasher.update(&[0xEA]);
    hasher.update(&validity.expires_at().as_unix_millis().to_le_bytes());

    hasher.update(&[0xEB, 0x00]);
    hasher.update(&[0xEC, evidence.optimizer_boundary().as_tag()]);

    hasher.finalize()
}
