#![forbid(unsafe_code)]

//! SRPL binder owner.
//!
//! This crate turns parsed SRPL AST shapes into bound procedure signatures for
//! lowering. It owns binder behavior without depending on the `andromeda-srpl`
//! facade.
//!
//! Dependency direction:
//! - consume parser and AST output from lower SRPL language-model crates;
//! - emit bound semantic data for lowering;
//! - avoid execution, storage, transaction, WAL, transport, benchmark,
//!   analytics, GPU, and application-surface dependencies.

mod catalog_plan;

use std::collections::BTreeSet;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl_ast::{ProcedureAst, ProcedureBodyAst};
use andromeda_srpl_ir::{ProcedureSignature, ResultContract};
use andromeda_types::ColumnDescriptor;

pub use catalog_plan::{
    SrplCatalogStructuredObjectBinding, SrplCatalogTableBinding, SrplExecutableCatalogView,
    bind_executable_procedure_plan, inventory_reserve_stock_body_ir,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundProcedure {
    pub signature: ProcedureSignature,
    pub body: ProcedureBodyAst,
}

pub fn bind_procedure(ast: ProcedureAst) -> AndromedaResult<BoundProcedure> {
    let accepts = ast
        .parameters
        .into_iter()
        .map(|field| ColumnDescriptor {
            name: field.name.value,
            data_type: field.data_type.value,
            ordinal: field.ordinal,
        })
        .collect::<Vec<_>>();

    let returns = ast
        .results
        .into_iter()
        .map(|result| ResultContract {
            name: result.name.value,
            cardinality: result.cardinality.value,
            columns: result
                .columns
                .into_iter()
                .map(|field| ColumnDescriptor {
                    name: field.name.value,
                    data_type: field.data_type.value,
                    ordinal: field.ordinal,
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    let bound = BoundProcedure {
        signature: ProcedureSignature {
            name: ast.name.value,
            accepts,
            returns,
        },
        body: ast.body,
    };

    validate_unique_names(&bound)?;
    bound.signature.validate()?;
    Ok(bound)
}

fn validate_unique_names(bound: &BoundProcedure) -> AndromedaResult<()> {
    let mut parameter_names = BTreeSet::new();
    for parameter in &bound.signature.accepts {
        if !parameter_names.insert(parameter.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL procedure input names must be unique",
            ));
        }
    }

    let mut result_names = BTreeSet::new();
    for result in &bound.signature.returns {
        if !result_names.insert(result.name.as_str()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "SRPL result stream names must be unique",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::QualifiedName;
    use andromeda_error::AndromedaErrorKind;
    use andromeda_srpl_ast::{Cardinality, FieldAst, ResultStreamAst, SourceSpan, Spanned};
    use andromeda_types::{ScalarType, TypeDescriptor};

    #[test]
    fn binder_owner_maps_ast_to_bound_signature_without_facade_dependency() {
        let bound = bind_procedure(procedure_ast(
            vec![
                field("ProductId", ScalarType::I64, 0),
                field("Quantity", ScalarType::I64, 1),
            ],
            vec![result_stream(
                "Reservation",
                Cardinality::One,
                vec![field("Reserved", ScalarType::Bool, 0)],
            )],
        ))
        .unwrap();

        assert_eq!(
            bound.signature.name.as_catalog_path(),
            "Inventory.ReserveStock"
        );
        assert_eq!(bound.signature.accepts.len(), 2);
        assert_eq!(bound.signature.accepts[0].name, "ProductId");
        assert_eq!(bound.signature.returns.len(), 1);
        assert_eq!(bound.signature.returns[0].columns[0].name, "Reserved");
        assert!(bound.body.operations.is_empty());
    }

    #[test]
    fn binder_owner_rejects_duplicate_parameter_names() {
        let error = bind_procedure(procedure_ast(
            vec![
                field("ProductId", ScalarType::I64, 0),
                field("ProductId", ScalarType::I64, 1),
            ],
            vec![result_stream(
                "Reservation",
                Cardinality::One,
                vec![field("Reserved", ScalarType::Bool, 0)],
            )],
        ))
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    }

    #[test]
    fn binder_owner_rejects_duplicate_result_stream_names() {
        let error = bind_procedure(procedure_ast(
            vec![field("ProductId", ScalarType::I64, 0)],
            vec![
                result_stream(
                    "Reservation",
                    Cardinality::One,
                    vec![field("Reserved", ScalarType::Bool, 0)],
                ),
                result_stream(
                    "Reservation",
                    Cardinality::OptionalOne,
                    vec![field("Accepted", ScalarType::Bool, 0)],
                ),
            ],
        ))
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Srpl);
    }

    fn procedure_ast(parameters: Vec<FieldAst>, results: Vec<ResultStreamAst>) -> ProcedureAst {
        ProcedureAst {
            name: Spanned::new(
                QualifiedName::parse("Inventory.ReserveStock").unwrap(),
                SourceSpan::new(0, 22),
            ),
            parameters,
            results,
            body: ProcedureBodyAst {
                operations: Vec::new(),
                span: SourceSpan::new(0, 0),
            },
            span: SourceSpan::new(0, 80),
        }
    }

    fn result_stream(
        name: &str,
        cardinality: Cardinality,
        columns: Vec<FieldAst>,
    ) -> ResultStreamAst {
        ResultStreamAst {
            name: Spanned::new(name.to_string(), SourceSpan::new(0, name.len())),
            cardinality: Spanned::new(cardinality, SourceSpan::new(0, 3)),
            columns,
            span: SourceSpan::new(0, 12),
        }
    }

    fn field(name: &str, scalar_type: ScalarType, ordinal: u32) -> FieldAst {
        FieldAst {
            name: Spanned::new(name.to_string(), SourceSpan::new(0, name.len())),
            data_type: Spanned::new(
                TypeDescriptor::required(scalar_type),
                SourceSpan::new(0, name.len()),
            ),
            ordinal,
        }
    }
}
