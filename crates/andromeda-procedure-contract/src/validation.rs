//! Compatibility checking between procedure contract versions.

use std::collections::BTreeSet;

use crate::ObjectKind;

use super::{
    AccessMode, CompatibilityDecision, CompatibilityPolicy, ContractCompatibilityDiagnostic,
    ProcedureContract,
};

/// Diagnose the compatibility relationship between two versions of the same
/// procedure contract.
///
/// # Decision classification
///
/// | Condition | Decision |
/// |---|---|
/// | Pre-flight: either contract fails canonical-hash validation | `Rejected` |
/// | Identity mismatch or non-advancing `CatalogVersion` | `Rejected` |
/// | `ExactHash` policy + different hash | `Breaking` |
/// | `AdditiveOnly` + required permission removed | `SecurityImpact` |
/// | `AdditiveOnly` + access mode widens `ReadOnly` → `ReadWrite` | `SecurityImpact` |
/// | `AdditiveOnly` + inputs / result-streams / policy fields changed | `Breaking` |
/// | No issues detected | `Additive` |
///
/// `SecurityImpact` takes priority over `Breaking` when both are detected in
/// the same `AdditiveOnly` evaluation.
///
/// `CompatibilityDecision::Deprecated` is **not** produced here; it is
/// reserved for `DefinitionOperation::Deprecate` (not yet implemented).
pub fn diagnose_procedure_contract_compatibility(
    previous: &ProcedureContract,
    next: &ProcedureContract,
) -> ContractCompatibilityDiagnostic {
    // ── Phase 1: pre-flight canonical-hash validation ──────────────────────
    let mut rejected_reasons = Vec::new();

    if let Err(error) = previous.validate_canonical_hash() {
        rejected_reasons.push(format!(
            "previous contract is invalid or non-canonical: {}",
            error.message()
        ));
    }
    if let Err(error) = next.validate_canonical_hash() {
        rejected_reasons.push(format!(
            "next contract is invalid or non-canonical: {}",
            error.message()
        ));
    }
    if !rejected_reasons.is_empty() {
        return ContractCompatibilityDiagnostic::with_decision(CompatibilityDecision::Rejected {
            reasons: rejected_reasons,
        });
    }

    // ── Phase 2: identity and version checks ───────────────────────────────
    let mut identity_reasons = Vec::new();

    if previous.object.name != next.object.name {
        identity_reasons
            .push("procedure contract compatibility requires the same procedure name".to_string());
    }
    if previous.procedure_id != next.procedure_id {
        identity_reasons
            .push("procedure contract compatibility requires the same ProcedureId".to_string());
    }
    if previous.object.object_id != next.object.object_id {
        identity_reasons.push(
            "procedure contract compatibility requires the same catalog object id".to_string(),
        );
    }
    if previous.object.kind != ObjectKind::Procedure || next.object.kind != ObjectKind::Procedure {
        identity_reasons.push(
            "procedure contract compatibility can only classify Procedure objects".to_string(),
        );
    }
    if next.object.catalog_version <= previous.object.catalog_version {
        identity_reasons.push(
            "procedure contract compatibility requires an advancing CatalogVersion".to_string(),
        );
    }
    if !identity_reasons.is_empty() {
        return ContractCompatibilityDiagnostic::with_decision(CompatibilityDecision::Rejected {
            reasons: identity_reasons,
        });
    }

    // ── Phase 3: policy-specific compatibility checks ──────────────────────
    match next.compatibility_policy {
        CompatibilityPolicy::ExactHash => {
            if previous.contract_hash != next.contract_hash {
                return ContractCompatibilityDiagnostic::with_decision(
                    CompatibilityDecision::Breaking {
                        reasons: vec![
                            "exact-hash compatibility requires unchanged contract hash".to_string(),
                        ],
                    },
                );
            }
        },

        CompatibilityPolicy::AdditiveOnly => {
            let mut security_reasons: Vec<String> = Vec::new();
            let mut breaking_reasons: Vec<String> = Vec::new();

            // Security: required permissions removed.
            let prev_perms: BTreeSet<&str> = previous
                .required_permissions
                .iter()
                .map(String::as_str)
                .collect();
            let next_perms: BTreeSet<&str> = next
                .required_permissions
                .iter()
                .map(String::as_str)
                .collect();
            let mut removed_perms: Vec<&str> =
                prev_perms.difference(&next_perms).copied().collect();
            removed_perms.sort_unstable();

            if !removed_perms.is_empty() {
                security_reasons.push(format!(
                    "required permissions removed under additive policy: {}",
                    removed_perms.join(", ")
                ));
            }

            // Security / Breaking: transaction policy changes.
            if previous.transaction_policy != next.transaction_policy {
                if previous.transaction_policy.access_mode == AccessMode::ReadOnly
                    && next.transaction_policy.access_mode == AccessMode::ReadWrite
                {
                    security_reasons.push(
                        "transaction access mode widens from ReadOnly to ReadWrite".to_string(),
                    );
                } else {
                    breaking_reasons.push(
                        "additive compatibility does not permit transaction policy changes"
                            .to_string(),
                    );
                }
            }

            // Breaking: permission changes that are not removals (additions /
            // reordering).
            if previous.required_permissions != next.required_permissions
                && removed_perms.is_empty()
            {
                breaking_reasons
                    .push("additive compatibility does not permit permission changes".to_string());
            }

            // Breaking: input shape.
            if previous.inputs != next.inputs {
                breaking_reasons
                    .push("additive compatibility does not permit input changes".to_string());
            }
            if previous.structured_inputs != next.structured_inputs {
                breaking_reasons.push(
                    "additive compatibility does not permit structured input changes".to_string(),
                );
            }
            if previous.stats_version != next.stats_version {
                breaking_reasons.push(
                    "additive compatibility does not permit stats version changes".to_string(),
                );
            }
            if previous.protocol_layout != next.protocol_layout {
                breaking_reasons.push(
                    "additive compatibility does not permit protocol layout changes".to_string(),
                );
            }
            if previous.result_metadata_policy != next.result_metadata_policy {
                breaking_reasons.push(
                    "additive compatibility does not permit result metadata policy changes"
                        .to_string(),
                );
            }
            if previous.error_policy != next.error_policy {
                breaking_reasons.push(
                    "additive compatibility does not permit error policy changes".to_string(),
                );
            }
            if previous.multi_result_policy != next.multi_result_policy {
                breaking_reasons.push(
                    "additive compatibility does not permit multi-result policy changes"
                        .to_string(),
                );
            }

            // Breaking: result stream regressions.
            for previous_stream in &previous.result_streams {
                let Some(next_stream) = next
                    .result_streams
                    .iter()
                    .find(|stream| stream.name == previous_stream.name)
                else {
                    breaking_reasons.push(format!(
                        "additive compatibility does not permit removing result stream {}",
                        previous_stream.name
                    ));
                    continue;
                };

                if previous_stream.stream_id != next_stream.stream_id {
                    breaking_reasons.push(format!(
                        "additive compatibility does not permit changing result stream id for result stream {}",
                        previous_stream.name
                    ));
                }

                if previous_stream.cardinality != next_stream.cardinality {
                    breaking_reasons.push(format!(
                        "additive compatibility does not permit changing cardinality contract for result stream {}",
                        previous_stream.name
                    ));
                }

                if next_stream.columns.len() < previous_stream.columns.len()
                    || !next_stream
                        .columns
                        .iter()
                        .zip(previous_stream.columns.iter())
                        .all(|(next_column, previous_column)| next_column == previous_column)
                {
                    breaking_reasons.push(format!(
                        "additive compatibility requires existing columns to remain an unchanged prefix for result stream {}",
                        previous_stream.name
                    ));
                }
            }

            // Priority: SecurityImpact > Breaking > Additive.
            if !security_reasons.is_empty() {
                return ContractCompatibilityDiagnostic::with_decision(
                    CompatibilityDecision::SecurityImpact {
                        reasons: security_reasons,
                    },
                );
            }
            if !breaking_reasons.is_empty() {
                return ContractCompatibilityDiagnostic::with_decision(
                    CompatibilityDecision::Breaking {
                        reasons: breaking_reasons,
                    },
                );
            }
        },
    }

    ContractCompatibilityDiagnostic::with_decision(CompatibilityDecision::Additive)
}
