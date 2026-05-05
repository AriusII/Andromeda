//! Bridge layer: SRPL source → DefinitionBatch procedure definitions.
//!
//! This module implements the integration between the SRPL compiler and the catalog
//! DefinitionBatch infrastructure. It provides:
//!
//! 1. **SrplProcedureDefinition**: A typed container for SRPL procedures at various
//!    compilation stages (source, AST, IR, manifest).
//!
//! 2. **Compilation Pipeline**: Parse → Bind → Lower → Manifest conversion.
//!
//! 3. **Catalog Conversion**: Transform compiled IR into a CatalogProcedureDefinition
//!    ready for inclusion in a DefinitionBatch.
//!
//! ## Contract
//!
//! - **Input**: SRPL source string
//! - **Output**: CatalogProcedureDefinition (serializable, versionable, hashable)
//! - **Properties**: Deterministic (same source → same IR → same contract hash)
//! - **Side Effects**: None (pure function)
//!
//! ## Error Handling
//!
//! Errors are categorized by compilation phase:
//! - **Syntax errors**: Lexer/parser failures
//! - **Bind errors**: Name/type validation failures
//! - **Compile errors**: Semantic validation failures
//! - **Manifest errors**: Contract materialization failures

use andromeda_catalog::{
    AccessMode, CatalogDefinition, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy,
    MultiResultPolicy, ObjectKind, ProcedureContractCandidate, ProcedureErrorPolicy,
    ProtocolLayoutRef, ResultMetadataPolicy, ResultStreamContract, StatsVersion, TransactionPolicy,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ColumnDescriptor, ContractHash, ProcedureId,
};

use crate::{
    ProcedureAst, SrplProcedureIr, lowering::lower_bound_procedure,
    procedure_compiler::parse_procedure_signature,
};

/// A staged SRPL procedure definition during compilation.
///
/// This type tracks the SRPL source through various compilation stages,
/// enabling diagnostics and error recovery at each phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureDefinition {
    /// The original SRPL source code
    pub srpl_source: String,

    /// Parsed AST (populated after lexer/parser stage)
    pub parsed_ast: Option<ProcedureAst>,

    /// Compiled IR (populated after binder/lowering stage)
    pub compiled_ir: Option<SrplProcedureIr>,
}

impl SrplProcedureDefinition {
    /// Create a new definition from SRPL source.
    pub fn from_source(srpl_source: String) -> Self {
        Self {
            srpl_source,
            parsed_ast: None,
            compiled_ir: None,
        }
    }

    /// Parse SRPL source to AST.
    ///
    /// This is the first compilation phase: lexer → parser.
    /// On success, populates `parsed_ast`. On failure, returns a diagnostic.
    pub fn parse(&mut self) -> AndromedaResult<()> {
        let ast = parse_procedure_signature(&self.srpl_source).map_err(|diagnostic| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!(
                    "SRPL syntax error during parsing: {} at {:?}",
                    diagnostic.message, diagnostic.location
                ),
            )
        })?;
        self.parsed_ast = Some(ast);
        Ok(())
    }

    /// Bind AST to typed procedure and lower to IR.
    ///
    /// This is the second compilation phase: bind → lower.
    /// Requires that `parse()` has completed successfully.
    /// On success, populates `compiled_ir`. On failure, returns a diagnostic.
    pub fn bind_and_lower(&mut self) -> AndromedaResult<()> {
        let ast = self.parsed_ast.clone().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "cannot bind and lower before parsing",
            )
        })?;

        let bound = crate::binder::bind_procedure(ast).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("SRPL binding error: {}", e),
            )
        })?;

        let ir = lower_bound_procedure(bound).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("SRPL lowering error: {}", e),
            )
        })?;

        self.compiled_ir = Some(ir);
        Ok(())
    }

    /// Convert compiled IR into a CatalogProcedureDefinition.
    ///
    /// This is the final phase: manifest generation and catalog conversion.
    /// Requires that `bind_and_lower()` has completed successfully.
    ///
    /// # Arguments
    ///
    /// * `object_id` - The catalog object ID for this procedure
    /// * `procedure_id` - The procedure ID for this procedure
    /// * `next_version` - The catalog version after this definition is applied
    /// * `database_id` - The database ID (for qualified naming)
    /// * `namespace_id` - The namespace ID (for qualified naming)
    ///
    /// # Returns
    ///
    /// A CatalogDefinition::Procedure with materialized contract.
    pub fn into_catalog_procedure_def(
        &self,
        object_id: CatalogObjectId,
        procedure_id: ProcedureId,
        next_version: CatalogVersion,
    ) -> AndromedaResult<CatalogDefinition> {
        let ir = self.compiled_ir.clone().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                "cannot materialize catalog definition before bind_and_lower",
            )
        })?;

        // Convert IR inputs to ColumnDescriptor
        let inputs = ir
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, col)| ColumnDescriptor {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                ordinal: idx as u32,
            })
            .collect::<Vec<_>>();

        // Convert IR result streams to ResultStreamContract
        let result_streams = ir
            .result_streams
            .iter()
            .enumerate()
            .map(|(idx, stream)| ResultStreamContract {
                stream_id: (idx as u64) + 1, // Stream IDs are 1-indexed
                name: stream.name.clone(),
                columns: stream
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(col_idx, col)| ColumnDescriptor {
                        name: col.name.clone(),
                        data_type: col.data_type.clone(),
                        ordinal: col_idx as u32,
                    })
                    .collect(),
                row_count_exact_required: false, // Default; can be customized by contract policy
            })
            .collect::<Vec<_>>();

        // Build contract candidate
        let candidate = ProcedureContractCandidate {
            object: CatalogObjectRef {
                object_id,
                name: ir.name.clone(),
                kind: ObjectKind::Procedure,
                catalog_version: next_version,
            },
            procedure_id,
            stats_version: StatsVersion::new(1), // Default stats version for new procedures
            protocol_layout: ProtocolLayoutRef {
                descriptor_set_hash: ContractHash::test_vector(0xA1), // Placeholder
                frame_envelope_hash: ContractHash::test_vector(0xA2), // Placeholder
            },
            inputs,
            structured_inputs: Vec::new(), // SRPL narrow procedures don't use structured inputs
            result_streams,
            required_permissions: vec!["procedure.Execute".to_string()], // Default permission
            transaction_policy: TransactionPolicy {
                access_mode: AccessMode::ReadWrite,
                isolation: IsolationPolicy::Serializable,
                retryable: false,
            },
            compatibility_policy: CompatibilityPolicy::ExactHash,
            result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
            error_policy: ProcedureErrorPolicy {
                rollback_on_error: true,
                allowed_error_codes: Vec::new(),
            },
            multi_result_policy: MultiResultPolicy::SingleResultOnly,
        };

        // Materialize contract (compute canonical hash)
        let contract = candidate.materialize()?;

        Ok(CatalogDefinition::Procedure(contract))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srpl_procedure_definition_from_source_creates_empty_stages() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let def = SrplProcedureDefinition::from_source(source.clone());

        assert_eq!(def.srpl_source, source);
        assert!(def.parsed_ast.is_none());
        assert!(def.compiled_ir.is_none());
    }

    #[test]
    fn srpl_procedure_definition_parse_populates_ast() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        assert!(def.parse().is_ok());
        assert!(def.parsed_ast.is_some());
    }

    #[test]
    fn srpl_procedure_definition_parse_syntax_error() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns R many ();"
            .to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        let result = def.parse();
        assert!(result.is_err());
    }

    #[test]
    fn srpl_procedure_definition_bind_and_lower_requires_parse() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        let result = def.bind_and_lower();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("before parsing"));
    }

    #[test]
    fn srpl_procedure_definition_bind_and_lower_populates_ir() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let mut def = SrplProcedureDefinition::from_source(source);

        assert!(def.parse().is_ok());
        assert!(def.bind_and_lower().is_ok());
        assert!(def.compiled_ir.is_some());
    }

    #[test]
    fn srpl_procedure_definition_into_catalog_requires_ir() {
        let source = "procedure Inventory.ReserveStock accepts (ProductId i64) returns Reservation one (Reserved bool);".to_string();
        let def = SrplProcedureDefinition::from_source(source);

        let result = def.into_catalog_procedure_def(
            CatalogObjectId::new(1),
            ProcedureId::new(1),
            CatalogVersion::new(1),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("before bind_and_lower")
        );
    }
}
