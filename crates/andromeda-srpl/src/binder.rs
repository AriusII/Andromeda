use std::collections::BTreeSet;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::ColumnDescriptor;

use crate::{ProcedureAst, ProcedureBodyAst, ProcedureSignature, ResultContract};

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
