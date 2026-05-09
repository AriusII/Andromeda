#![forbid(unsafe_code)]

//! SRPL execution adapter contracts.
//!
//! This crate defines the narrow boundary between catalog-bound SRPL plans and
//! concrete execution owners. It intentionally carries no storage, transport,
//! transaction, WAL, parser, or compatibility-surface dependency.
//!
//! Dependency direction:
//! - define runtime-free adapter contracts over typed SRPL IR and Procedure
//!   contract shapes;
//! - allow concrete execution owners to depend on this boundary;
//! - avoid parser, binder, lowering, optimizer, storage, transaction, WAL,
//!   transport, benchmark, analytics, GPU, and application-surface
//!   dependencies.

mod contracts;
mod diagnostics;
mod environment;
mod results;
mod traits;

pub use contracts::{
    SrplAssertRequest, SrplEmitRequest, SrplFailureRequest, SrplOperationContext, SrplReadRequest,
    SrplRowBound, SrplUpdateRequest,
};
pub use diagnostics::SrplExecutionFailure;
pub use environment::{SrplBindingEnvironment, SrplBoundRow, SrplBoundValue};
pub use results::{SrplAssertResult, SrplEmitResult, SrplReadResult, SrplUpdateResult};
pub use traits::{
    SrplAssertionAdapter, SrplFailureAdapter, SrplTypedEmitAdapter, SrplTypedReadAdapter,
    SrplTypedUpdateAdapter,
};

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_contract::{CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName};
    use andromeda_error::AndromedaErrorKind;
    use andromeda_srpl_ir::{Cardinality, SrplAssignmentIr, SrplEmitValueIr, SrplPredicateIr};
    use andromeda_types::{
        CatalogObjectId, CatalogVersion, ContractHash, ProcedureId, ScalarType, TypeDescriptor,
    };

    fn procedure() -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: ProcedureId::new(11),
            contract_hash: ContractHash::test_vector(0x11),
            catalog_version: CatalogVersion::new(7),
        }
    }

    fn context(ordinal: u32) -> SrplOperationContext {
        SrplOperationContext::new(procedure(), ordinal).unwrap()
    }

    fn catalog_object(kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(22),
            name: QualifiedName::parse("Inventory.Stock").unwrap(),
            kind,
            catalog_version: CatalogVersion::new(7),
        }
    }

    #[test]
    fn execution_adapter_bounded_read_request_constructs() {
        let bound = SrplRowBound::at_most(8).unwrap();
        let request = SrplReadRequest::new(
            context(0),
            catalog_object(ObjectKind::Table),
            Cardinality::Many,
            bound,
            vec![SrplPredicateIr::InputEqualsField {
                input: "ProductId".to_string(),
                binding: "stock".to_string(),
                field: "ProductId".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(request.row_bound.get(), 8);
        assert_eq!(request.source.kind, ObjectKind::Table);
    }

    #[test]
    fn execution_adapter_bounded_update_request_constructs() {
        let request = SrplUpdateRequest::new(
            context(1),
            catalog_object(ObjectKind::Table),
            SrplRowBound::exact(1).unwrap(),
            Vec::new(),
            vec![SrplAssignmentIr {
                field: "Reserved".to_string(),
                value: andromeda_srpl_ir::SrplValueIr::bool(true),
            }],
        )
        .unwrap();

        assert!(request.affected_rows.is_exact());
        assert_eq!(request.assignments.len(), 1);
    }

    #[test]
    fn execution_adapter_assert_request_constructs() {
        let request = SrplAssertRequest::new(
            context(1),
            SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding: "stock".to_string(),
                field: "Available".to_string(),
                input: "Quantity".to_string(),
            },
            "InsufficientStock",
        )
        .unwrap();

        assert_eq!(request.failure_code, "InsufficientStock");
        assert_eq!(request.context.ordinal, 1);
    }

    #[test]
    fn execution_adapter_bounded_emit_request_constructs() {
        let request = SrplEmitRequest::new(
            context(2),
            "Reservation",
            Cardinality::One,
            SrplRowBound::exact(1).unwrap(),
            vec![SrplEmitValueIr {
                column: "Reserved".to_string(),
                value: andromeda_srpl_ir::SrplValueIr::bool(true),
            }],
        )
        .unwrap();

        assert_eq!(request.stream, "Reservation");
        assert_eq!(request.row_bound.get(), 1);
    }

    #[test]
    fn execution_adapter_rejects_unbounded_and_zero_bounds() {
        assert_eq!(
            SrplRowBound::exact(0).unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
        assert_eq!(
            SrplRowBound::at_most(0).unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
        assert_eq!(
            SrplRowBound::required_at_most(None).unwrap_err().kind(),
            AndromedaErrorKind::Srpl
        );
    }

    #[test]
    fn execution_adapter_rejects_non_table_read_target() {
        let error = SrplReadRequest::new(
            context(0),
            catalog_object(ObjectKind::Procedure),
            Cardinality::Many,
            SrplRowBound::at_most(1).unwrap(),
            Vec::new(),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
    }

    #[test]
    fn execution_adapter_rejects_zero_procedure_binding() {
        let procedure = ProcedureContractRef {
            procedure_id: ProcedureId::new(0),
            contract_hash: ContractHash::test_vector(0x11),
            catalog_version: CatalogVersion::new(7),
        };

        assert_eq!(
            SrplOperationContext::new(procedure, 0).unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn execution_adapter_results_map_bound_failures_without_panic() {
        let bound = SrplRowBound::exact(1).unwrap();

        let failure = SrplReadResult::<()>::new(vec![(), ()], Cardinality::One, bound).unwrap_err();

        assert!(matches!(
            failure.clone(),
            SrplExecutionFailure::CardinalityViolation {
                expected: Cardinality::One,
                actual_rows: 2,
                ..
            }
        ));
        assert_eq!(
            failure.into_andromeda_error().kind(),
            AndromedaErrorKind::Execution
        );
    }

    #[test]
    fn execution_adapter_keeps_core_types_only_for_shape_tests() {
        let descriptor = TypeDescriptor::required(ScalarType::Bool);

        assert!(descriptor.validate().is_ok());
    }
}
