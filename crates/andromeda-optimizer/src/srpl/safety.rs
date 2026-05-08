use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl_ir::{
    Cardinality, SrplBusinessOperationKindIr, SrplPredicateIr, SrplProcedureIr,
};

pub(crate) fn ensure_effect_surface_preserved(
    before: &SrplProcedureIr,
    after: &SrplProcedureIr,
    phase: &str,
) -> AndromedaResult<()> {
    let before_markers = effect_surface_markers(before);
    let after_markers = effect_surface_markers(after);
    if before_markers == after_markers {
        return Ok(());
    }

    Err(AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!("SRPL optimizer phase {phase} changed the effectful operation surface"),
    ))
}

fn effect_surface_markers(ir: &SrplProcedureIr) -> Vec<EffectSurfaceMarker> {
    ir.body
        .operations
        .iter()
        .map(|operation| match &operation.kind {
            SrplBusinessOperationKindIr::Read {
                source,
                binding,
                cardinality,
                ..
            } => EffectSurfaceMarker::Read {
                source: source.as_catalog_path(),
                binding: binding.clone(),
                cardinality: *cardinality,
            },
            SrplBusinessOperationKindIr::Assert {
                predicate,
                failure_code,
            } => EffectSurfaceMarker::Assert {
                predicate: predicate.clone(),
                failure_code: failure_code.clone(),
            },
            SrplBusinessOperationKindIr::Update {
                target,
                assignments,
                affected_rows_exact,
                ..
            } => EffectSurfaceMarker::Update {
                target: target.as_catalog_path(),
                assignment_fields: assignments
                    .iter()
                    .map(|assignment| assignment.field.clone())
                    .collect(),
                affected_rows_exact: *affected_rows_exact,
            },
            SrplBusinessOperationKindIr::Emit { stream, values } => EffectSurfaceMarker::Emit {
                stream: stream.clone(),
                columns: values.iter().map(|value| value.column.clone()).collect(),
            },
            SrplBusinessOperationKindIr::Raise { code } => {
                EffectSurfaceMarker::Raise { code: code.clone() }
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EffectSurfaceMarker {
    Read {
        source: String,
        binding: String,
        cardinality: Cardinality,
    },
    Assert {
        predicate: SrplPredicateIr,
        failure_code: String,
    },
    Update {
        target: String,
        assignment_fields: Vec<String>,
        affected_rows_exact: Option<u64>,
    },
    Emit {
        stream: String,
        columns: Vec<String>,
    },
    Raise {
        code: String,
    },
}
