#![forbid(unsafe_code)]

//! Shared deterministic SRPL test fixtures.
//!
//! This crate owns reusable test-only fixture shapes for SRPL binder,
//! interpreter, adapter, and facade compatibility tests. It does not execute
//! production behavior or introduce storage, WAL, transaction, transport, SQL,
//! benchmark, analytics, or GPU dependencies.

use andromeda_contract::{CatalogObjectRef, ObjectKind, ProcedureContractRef, QualifiedName};
use andromeda_srpl_cardinality::Cardinality;
use andromeda_srpl_execution_adapter::{
    SrplAssertRequest, SrplAssertResult, SrplAssertionAdapter, SrplBindingEnvironment,
    SrplEmitRequest, SrplEmitResult, SrplExecutionFailure, SrplFailureAdapter, SrplFailureRequest,
    SrplReadRequest, SrplReadResult, SrplTypedEmitAdapter, SrplTypedReadAdapter,
    SrplTypedUpdateAdapter, SrplUpdateRequest, SrplUpdateResult,
};
use andromeda_srpl_ir::{
    BoundSrplBodyPlan, BoundSrplOperationPlan, ExecutableProcedurePlan, SrplAssignmentIr,
    SrplCatalogBindingEvidence, SrplEmitValueIr, SrplObjectBindingEvidence, SrplPredicateIr,
    SrplValueIr,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

#[derive(Default)]
pub struct FakeSrplAdapter {
    pub events: Vec<String>,
    pub assert_passes: bool,
    pub read_rows: usize,
    pub affected_rows: u64,
    pub emitted_rows: u64,
}

impl FakeSrplAdapter {
    pub fn passing() -> Self {
        Self {
            assert_passes: true,
            read_rows: 1,
            affected_rows: 1,
            emitted_rows: 1,
            events: Vec::new(),
        }
    }
}

impl SrplTypedReadAdapter for FakeSrplAdapter {
    type Row = ();

    fn read_typed(
        &mut self,
        request: SrplReadRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplReadResult<Self::Row>, SrplExecutionFailure> {
        self.events
            .push(format!("read:{}", request.context.ordinal));
        SrplReadResult::new(
            vec![(); self.read_rows],
            request.cardinality,
            request.row_bound,
        )
    }
}

impl SrplAssertionAdapter for FakeSrplAdapter {
    fn assert_typed(
        &mut self,
        request: SrplAssertRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplAssertResult, SrplExecutionFailure> {
        self.events
            .push(format!("assert:{}", request.context.ordinal));
        Ok(SrplAssertResult::new(self.assert_passes))
    }
}

impl SrplTypedUpdateAdapter for FakeSrplAdapter {
    fn update_typed(
        &mut self,
        request: SrplUpdateRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplUpdateResult, SrplExecutionFailure> {
        self.events
            .push(format!("update:{}", request.context.ordinal));
        SrplUpdateResult::new(self.affected_rows, request.affected_rows)
    }
}

impl SrplTypedEmitAdapter for FakeSrplAdapter {
    fn emit_typed(
        &mut self,
        request: SrplEmitRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<SrplEmitResult, SrplExecutionFailure> {
        self.events
            .push(format!("emit:{}", request.context.ordinal));
        SrplEmitResult::new(self.emitted_rows, request.cardinality, request.row_bound)
    }
}

impl SrplFailureAdapter for FakeSrplAdapter {
    fn fail_typed(
        &mut self,
        request: SrplFailureRequest,
        _environment: &dyn SrplBindingEnvironment,
    ) -> Result<(), SrplExecutionFailure> {
        self.events
            .push(format!("fail:{}", request.context.ordinal));
        Ok(())
    }
}

pub fn srpl_procedure_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(7),
        contract_hash: ContractHash::test_vector(0xA7),
        catalog_version: CatalogVersion::new(3),
    }
}

pub fn srpl_procedure_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(99),
        name: fixture_name("Inventory.ReserveStock"),
        kind: ObjectKind::Procedure,
        catalog_version: CatalogVersion::new(3),
    }
}

pub fn srpl_stock_object() -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(100),
        name: fixture_name("Inventory.ProductStock"),
        kind: ObjectKind::Table,
        catalog_version: CatalogVersion::new(3),
    }
}

pub fn srpl_predicate() -> SrplPredicateIr {
    SrplPredicateIr::InputEqualsField {
        input: "ProductId".to_string(),
        binding: "Stock".to_string(),
        field: "ProductId".to_string(),
    }
}

pub fn srpl_assignment() -> SrplAssignmentIr {
    SrplAssignmentIr {
        field: "AvailableQuantity".to_string(),
        value: SrplValueIr::Bool(true),
    }
}

pub fn srpl_emit_value() -> SrplEmitValueIr {
    SrplEmitValueIr {
        column: "Reserved".to_string(),
        value: SrplValueIr::Bool(true),
    }
}

pub fn srpl_plan(operations: Vec<BoundSrplOperationPlan>) -> ExecutableProcedurePlan {
    ExecutableProcedurePlan {
        procedure_name: fixture_name("Inventory.ReserveStock"),
        body: BoundSrplBodyPlan { operations },
        evidence: SrplCatalogBindingEvidence {
            catalog_version: CatalogVersion::new(3),
            procedure_object: srpl_procedure_object(),
            procedure_contract: srpl_procedure_ref(),
            bound_objects: vec![SrplObjectBindingEvidence {
                object: srpl_stock_object(),
                shape_hash: ContractHash::test_vector(0xC1),
                kind: ObjectKind::Table,
            }],
        },
    }
}

pub fn srpl_happy_path_plan() -> ExecutableProcedurePlan {
    srpl_plan(vec![
        BoundSrplOperationPlan::ReadTable {
            ordinal: 0,
            source: srpl_stock_object(),
            binding: "Stock".to_string(),
            cardinality: Cardinality::One,
            predicates: vec![srpl_predicate()],
        },
        BoundSrplOperationPlan::Assert {
            ordinal: 1,
            predicate: SrplPredicateIr::FieldGreaterThanOrEqualInput {
                binding: "Stock".to_string(),
                field: "AvailableQuantity".to_string(),
                input: "Quantity".to_string(),
            },
            failure_code: "InsufficientStock".to_string(),
        },
        BoundSrplOperationPlan::UpdateTable {
            ordinal: 2,
            target: srpl_stock_object(),
            predicates: vec![srpl_predicate()],
            assignments: vec![srpl_assignment()],
            affected_rows_exact: Some(1),
        },
        BoundSrplOperationPlan::Emit {
            ordinal: 3,
            stream: "Reservation".to_string(),
            values: vec![srpl_emit_value()],
        },
    ])
}

fn fixture_name(value: &str) -> QualifiedName {
    QualifiedName::parse(value).expect("static SRPL fixture qualified name must parse")
}
