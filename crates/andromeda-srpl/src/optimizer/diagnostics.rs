use crate::{
    SrplBusinessOperationKindIr, SrplProcedureIr,
    optimizer::{
        phase::OptimizerPhase, pipeline::OptimizationLevel,
        projection_pushdown::EffectiveProjection,
    },
};

/// Stable class of optimizer decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizerDecisionKind {
    /// A pass was skipped because the requested optimization level disables it.
    PassSkipped,
    /// A value expression was folded to a compile-time constant.
    ConstantValueFolded,
    /// An assertion predicate was moved into an upstream read.
    PredicatePushedToRead,
    /// Predicate ordering or deduplication changed during canonicalization.
    PredicateNormalized,
    /// An operation ordinal changed while preserving operation identity.
    OperationOrdinalRewritten,
    /// A scan projection was computed for a read binding.
    ProjectionComputed,
    /// The optimizer selected the final plan candidate.
    PlanChosen,
}

/// Structured optimizer diagnostic/provenance entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizerDiagnostic {
    /// Phase that produced the diagnostic.
    pub phase: OptimizerPhase,
    /// Decision class.
    pub decision: OptimizerDecisionKind,
    /// Original IR operation ordinal, when the decision originates from one.
    pub source_operation_ordinal: Option<u32>,
    /// Resulting IR operation ordinal, when the decision maps to one.
    pub resulting_operation_ordinal: Option<u32>,
    /// Human-readable diagnostic detail.
    pub message: String,
}

impl OptimizerDiagnostic {
    pub(crate) fn new(
        phase: OptimizerPhase,
        decision: OptimizerDecisionKind,
        source_operation_ordinal: Option<u32>,
        resulting_operation_ordinal: Option<u32>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            decision,
            source_operation_ordinal,
            resulting_operation_ordinal,
            message: message.into(),
        }
    }
}

pub(crate) fn skipped_pass_diagnostic(
    phase: OptimizerPhase,
    level: OptimizationLevel,
) -> OptimizerDiagnostic {
    OptimizerDiagnostic::new(
        phase,
        OptimizerDecisionKind::PassSkipped,
        None,
        None,
        format!("{phase:?} skipped at optimization level {level:?}"),
    )
}

pub(crate) fn constant_folding_diagnostics(
    before: &SrplProcedureIr,
    after: &SrplProcedureIr,
) -> Vec<OptimizerDiagnostic> {
    before
        .body
        .operations
        .iter()
        .zip(after.body.operations.iter())
        .filter_map(|(before_op, after_op)| {
            let changed = match (&before_op.kind, &after_op.kind) {
                (
                    SrplBusinessOperationKindIr::Update {
                        assignments: before,
                        ..
                    },
                    SrplBusinessOperationKindIr::Update {
                        assignments: after, ..
                    },
                ) => before != after,
                (
                    SrplBusinessOperationKindIr::Emit { values: before, .. },
                    SrplBusinessOperationKindIr::Emit { values: after, .. },
                ) => before != after,
                _ => false,
            };
            changed.then(|| {
                OptimizerDiagnostic::new(
                    OptimizerPhase::ConstantFolding,
                    OptimizerDecisionKind::ConstantValueFolded,
                    Some(before_op.ordinal),
                    Some(after_op.ordinal),
                    "folded deterministic constant value expression",
                )
            })
        })
        .collect()
}

pub(crate) fn predicate_pushdown_diagnostics(
    before: &SrplProcedureIr,
    after: &SrplProcedureIr,
) -> Vec<OptimizerDiagnostic> {
    let mut diagnostics = Vec::new();
    for before_op in &before.body.operations {
        let SrplBusinessOperationKindIr::Assert { predicate, .. } = &before_op.kind else {
            continue;
        };
        let still_present = after.body.operations.iter().any(|after_op| {
            matches!(
                &after_op.kind,
                SrplBusinessOperationKindIr::Assert {
                    predicate: after_predicate,
                    ..
                } if after_predicate == predicate
            )
        });
        if still_present {
            continue;
        }

        if let Some(read_op) = after.body.operations.iter().find(|after_op| {
            matches!(
                &after_op.kind,
                SrplBusinessOperationKindIr::Read { predicates, .. }
                    if predicates.contains(predicate)
            )
        }) {
            diagnostics.push(OptimizerDiagnostic::new(
                OptimizerPhase::PredicatePushdown,
                OptimizerDecisionKind::PredicatePushedToRead,
                Some(before_op.ordinal),
                Some(read_op.ordinal),
                "pushed assertion predicate into upstream read",
            ));
        }
    }
    diagnostics
}

pub(crate) fn normalize_diagnostics(
    before: &SrplProcedureIr,
    after: &SrplProcedureIr,
) -> Vec<OptimizerDiagnostic> {
    let mut diagnostics = Vec::new();
    for (before_op, after_op) in before
        .body
        .operations
        .iter()
        .zip(after.body.operations.iter())
    {
        if before_op.ordinal != after_op.ordinal {
            diagnostics.push(OptimizerDiagnostic::new(
                OptimizerPhase::Normalize,
                OptimizerDecisionKind::OperationOrdinalRewritten,
                Some(before_op.ordinal),
                Some(after_op.ordinal),
                "rewrote operation ordinal while preserving operation identity",
            ));
        }

        if let (
            SrplBusinessOperationKindIr::Read {
                predicates: before, ..
            },
            SrplBusinessOperationKindIr::Read {
                predicates: after, ..
            },
        ) = (&before_op.kind, &after_op.kind)
            && before != after
        {
            diagnostics.push(OptimizerDiagnostic::new(
                OptimizerPhase::Normalize,
                OptimizerDecisionKind::PredicateNormalized,
                Some(before_op.ordinal),
                Some(after_op.ordinal),
                "canonicalized read predicates",
            ));
        }
    }
    diagnostics
}

pub(crate) fn projection_diagnostics(
    ir: &SrplProcedureIr,
    projections: &[EffectiveProjection],
) -> Vec<OptimizerDiagnostic> {
    projections
        .iter()
        .filter_map(|projection| {
            let ordinal =
                ir.body
                    .operations
                    .iter()
                    .find_map(|operation| match &operation.kind {
                        SrplBusinessOperationKindIr::Read { binding, .. }
                            if binding == &projection.binding =>
                        {
                            Some(operation.ordinal)
                        }
                        _ => None,
                    })?;
            Some(OptimizerDiagnostic::new(
                OptimizerPhase::ProjectionPushdown,
                OptimizerDecisionKind::ProjectionComputed,
                Some(ordinal),
                Some(ordinal),
                format!(
                    "computed projection for read binding {}",
                    projection.binding
                ),
            ))
        })
        .collect()
}
