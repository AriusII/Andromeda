use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ColumnDescriptor,
    ContractHash, DecimalType, FloatMode, FloatType, ProcedureId, ScalarType, TextEncoding,
    TimestampType,
};

use crate::{
    names::QualifiedName, objects::{validate_columns, validate_columns_allow_empty},
    CatalogObjectRef,
    ObjectKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureContractRef {
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl ProcedureContractRef {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure id must not be zero",
            ));
        }

        if self.catalog_version.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure catalog version must not be zero",
            ));
        }

        if self.contract_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure contract hash must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationPolicy {
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionPolicy {
    pub access_mode: AccessMode,
    pub isolation: IsolationPolicy,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityPolicy {
    AdditiveOnly,
    ExactHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCompatibilityDiagnostic {
    pub compatible: bool,
    pub messages: Vec<String>,
}

impl ContractCompatibilityDiagnostic {
    pub fn compatible() -> Self {
        Self {
            compatible: true,
            messages: Vec::new(),
        }
    }

    pub fn incompatible(messages: Vec<String>) -> Self {
        Self {
            compatible: false,
            messages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultStreamContract {
    pub name: String,
    pub columns: Vec<ColumnDescriptor>,
    pub row_count_exact_required: bool,
}

impl ResultStreamContract {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream name must not be empty",
            ));
        }

        validate_columns(&self.columns)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureContract {
    pub object: CatalogObjectRef,
    pub procedure_id: ProcedureId,
    pub contract_hash: ContractHash,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
}

impl ProcedureContract {
    pub fn as_ref(&self) -> ProcedureContractRef {
        ProcedureContractRef {
            procedure_id: self.procedure_id,
            contract_hash: self.contract_hash,
            catalog_version: self.object.catalog_version,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.object.validate_for_definition(ObjectKind::Procedure)?;
        self.as_ref().validate()?;
        validate_columns_allow_empty(&self.inputs)?;

        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure contract must declare required permissions",
            ));
        }
        let mut permissions = std::collections::BTreeSet::new();
        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure required permissions must not be empty",
                ));
            }
            if !permissions.insert(permission.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure required permissions must be unique",
                ));
            }
        }

        let mut structured_input_names = std::collections::BTreeSet::new();
        for structured_input in &self.structured_inputs {
            if !structured_input_names.insert(structured_input) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure structured input names must be unique",
                ));
            }
        }

        let mut result_stream_names = std::collections::BTreeSet::new();
        for stream in &self.result_streams {
            stream.validate()?;
            if !result_stream_names.insert(stream.name.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure result stream names must be unique",
                ));
            }
        }

        Ok(())
    }

    pub fn canonical_hash(&self) -> ContractHash {
        canonical_procedure_contract_hash(self)
    }

    pub fn validate_canonical_hash(&self) -> AndromedaResult<()> {
        self.validate()?;
        if self.contract_hash != self.canonical_hash() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure contract hash must match canonical contract shape",
            ));
        }

        Ok(())
    }

    pub fn validated(self) -> AndromedaResult<Self> {
        self.validate()?;
        Ok(self)
    }

    pub fn compatibility_with(
        &self,
        previous: &ProcedureContract,
    ) -> ContractCompatibilityDiagnostic {
        diagnose_procedure_contract_compatibility(previous, self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureContractCandidate {
    pub object: CatalogObjectRef,
    pub procedure_id: ProcedureId,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
}

impl ProcedureContractCandidate {
    pub fn materialize(self) -> AndromedaResult<ProcedureContract> {
        let contract_hash = canonical_procedure_contract_hash_parts(
            &self.object.name,
            &self.inputs,
            &self.structured_inputs,
            &self.result_streams,
            &self.required_permissions,
            self.transaction_policy,
            self.compatibility_policy,
        );
        let contract = ProcedureContract {
            object: self.object,
            procedure_id: self.procedure_id,
            contract_hash,
            inputs: self.inputs,
            structured_inputs: self.structured_inputs,
            result_streams: self.result_streams,
            required_permissions: self.required_permissions,
            transaction_policy: self.transaction_policy,
            compatibility_policy: self.compatibility_policy,
        };
        contract.validate_canonical_hash()?;
        Ok(contract)
    }
}

pub fn diagnose_procedure_contract_compatibility(
    previous: &ProcedureContract,
    next: &ProcedureContract,
) -> ContractCompatibilityDiagnostic {
    let mut messages = Vec::new();

    if let Err(error) = previous.validate() {
        messages.push(format!("previous contract is invalid: {}", error.message()));
    }
    if let Err(error) = next.validate() {
        messages.push(format!("next contract is invalid: {}", error.message()));
    }
    if !messages.is_empty() {
        return ContractCompatibilityDiagnostic::incompatible(messages);
    }

    if previous.object.name != next.object.name {
        messages
            .push("procedure contract compatibility requires the same procedure name".to_string());
    }

    match next.compatibility_policy {
        CompatibilityPolicy::ExactHash => {
            if previous.contract_hash != next.contract_hash {
                messages
                    .push("exact-hash compatibility requires unchanged contract hash".to_string());
            }
        }
        CompatibilityPolicy::AdditiveOnly => {
            if previous.inputs != next.inputs {
                messages.push("additive compatibility does not permit input changes".to_string());
            }
            if previous.structured_inputs != next.structured_inputs {
                messages.push(
                    "additive compatibility does not permit structured input changes".to_string(),
                );
            }
            if previous.required_permissions != next.required_permissions {
                messages
                    .push("additive compatibility does not permit permission changes".to_string());
            }
            if previous.transaction_policy != next.transaction_policy {
                messages.push(
                    "additive compatibility does not permit transaction policy changes".to_string(),
                );
            }
            for previous_stream in &previous.result_streams {
                let Some(next_stream) = next
                    .result_streams
                    .iter()
                    .find(|stream| stream.name == previous_stream.name)
                else {
                    messages.push(format!(
                        "additive compatibility does not permit removing result stream {}",
                        previous_stream.name
                    ));
                    continue;
                };

                if previous_stream.row_count_exact_required != next_stream.row_count_exact_required
                {
                    messages.push(format!(
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
                    messages.push(format!(
                        "additive compatibility requires existing columns to remain an unchanged prefix for result stream {}",
                        previous_stream.name
                    ));
                }
            }
        }
    }

    if messages.is_empty() {
        ContractCompatibilityDiagnostic::compatible()
    } else {
        ContractCompatibilityDiagnostic::incompatible(messages)
    }
}

fn canonical_procedure_contract_hash(contract: &ProcedureContract) -> ContractHash {
    canonical_procedure_contract_hash_parts(
        &contract.object.name,
        &contract.inputs,
        &contract.structured_inputs,
        &contract.result_streams,
        &contract.required_permissions,
        contract.transaction_policy,
        contract.compatibility_policy,
    )
}

fn canonical_procedure_contract_hash_parts(
    name: &QualifiedName,
    inputs: &[ColumnDescriptor],
    structured_inputs: &[QualifiedName],
    result_streams: &[ResultStreamContract],
    required_permissions: &[String],
    transaction_policy: TransactionPolicy,
    compatibility_policy: CompatibilityPolicy,
) -> ContractHash {
    let mut sink = StableHashSink::new();
    sink.str("andromeda.catalog.procedure-contract.v1");
    sink.qualified_name(name);
    sink.columns(inputs);
    sink.u64(structured_inputs.len() as u64);
    for structured_input in structured_inputs {
        sink.qualified_name(structured_input);
    }
    sink.u64(result_streams.len() as u64);
    for stream in result_streams {
        sink.str(&stream.name);
        sink.bool(stream.row_count_exact_required);
        sink.columns(&stream.columns);
    }
    sink.u64(required_permissions.len() as u64);
    for permission in required_permissions {
        sink.str(permission);
    }
    sink.u8(match transaction_policy.access_mode {
        AccessMode::ReadOnly => 0,
        AccessMode::ReadWrite => 1,
    });
    sink.u8(match transaction_policy.isolation {
        IsolationPolicy::Snapshot => 0,
        IsolationPolicy::Serializable => 1,
    });
    sink.bool(transaction_policy.retryable);
    sink.u8(match compatibility_policy {
        CompatibilityPolicy::AdditiveOnly => 0,
        CompatibilityPolicy::ExactHash => 1,
    });
    sink.finish()
}

struct StableHashSink {
    lanes: [u64; 4],
}

impl StableHashSink {
    fn new() -> Self {
        Self {
            lanes: [
                0xcbf29ce484222325,
                0x9e3779b97f4a7c15,
                0x517cc1b727220a95,
                0x6a09e667f3bcc909,
            ],
        }
    }

    fn finish(self) -> ContractHash {
        let mut bytes = [0u8; ContractHash::LEN];
        for (lane_index, lane) in self.lanes.into_iter().enumerate() {
            bytes[lane_index * 8..(lane_index + 1) * 8].copy_from_slice(&lane.to_le_bytes());
        }
        ContractHash::new(bytes)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.u8(*byte);
        }
    }

    fn u8(&mut self, value: u8) {
        for (lane_index, lane) in self.lanes.iter_mut().enumerate() {
            *lane ^= u64::from(value).wrapping_add((lane_index as u64) << 8);
            *lane = lane.wrapping_mul(0x100000001b3 + (lane_index as u64 * 0x1000003d));
            *lane ^= lane.rotate_left(17 + lane_index as u32);
        }
    }

    fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn qualified_name(&mut self, name: &QualifiedName) {
        self.u64(name.parts().len() as u64);
        for part in name.parts() {
            self.str(part);
        }
    }

    fn columns(&mut self, columns: &[ColumnDescriptor]) {
        self.u64(columns.len() as u64);
        for column in columns {
            self.str(&column.name);
            self.type_descriptor(&column.data_type);
            self.u32(column.ordinal);
        }
    }

    fn type_descriptor(&mut self, descriptor: &andromeda_core::TypeDescriptor) {
        self.scalar_type(&descriptor.scalar);
        self.u8(match descriptor.absence {
            andromeda_core::AbsencePolicy::Required => 0,
            andromeda_core::AbsencePolicy::ExplicitOptional => 1,
        });
    }

    fn scalar_type(&mut self, scalar: &ScalarType) {
        match scalar {
            ScalarType::I8 => self.u8(0),
            ScalarType::I16 => self.u8(1),
            ScalarType::I32 => self.u8(2),
            ScalarType::I64 => self.u8(3),
            ScalarType::I128 => self.u8(4),
            ScalarType::U8 => self.u8(5),
            ScalarType::U16 => self.u8(6),
            ScalarType::U32 => self.u8(7),
            ScalarType::U64 => self.u8(8),
            ScalarType::U128 => self.u8(9),
            ScalarType::Decimal(decimal) => {
                self.u8(10);
                self.decimal_type(*decimal);
            }
            ScalarType::Float(float) => {
                self.u8(11);
                self.float_type(*float);
            }
            ScalarType::Bool => self.u8(12),
            ScalarType::Text(text) => {
                self.u8(13);
                self.u8(match text.encoding {
                    TextEncoding::Utf8 => 0,
                    TextEncoding::Utf16 => 1,
                    TextEncoding::Unicode => 2,
                });
                self.u32(text.max_length.unwrap_or(0));
                self.bool(text.max_length.is_some());
                match &text.collation {
                    Some(collation) => {
                        self.bool(true);
                        self.str(collation);
                    }
                    None => self.bool(false),
                }
            }
            ScalarType::Timestamp(timestamp) => {
                self.u8(14);
                self.u8(match timestamp {
                    TimestampType::Transaction => 0,
                    TimestampType::Invocation => 1,
                    TimestampType::MonotonicEpoch => 2,
                });
            }
        }
    }

    fn decimal_type(&mut self, decimal: DecimalType) {
        match decimal {
            DecimalType::Min => self.u8(0),
            DecimalType::Mid => self.u8(1),
            DecimalType::Max => self.u8(2),
            DecimalType::Custom { precision, scale } => {
                self.u8(3);
                self.u8(precision);
                self.u8(scale);
            }
        }
    }

    fn float_type(&mut self, float: FloatType) {
        match float {
            FloatType::Min => self.u8(0),
            FloatType::Mid => self.u8(1),
            FloatType::Max => self.u8(2),
            FloatType::Custom { bits, mode } => {
                self.u8(3);
                self.raw_bytes(&bits.to_le_bytes());
                self.u8(match mode {
                    FloatMode::Approximate => 0,
                    FloatMode::DeterministicAnalytics => 1,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CatalogObjectRef, ObjectKind, QualifiedName};
    use andromeda_core::{CatalogObjectId, ColumnDescriptor, ScalarType, TypeDescriptor};

    fn object(kind: ObjectKind) -> CatalogObjectRef {
        CatalogObjectRef {
            object_id: CatalogObjectId::new(1),
            name: QualifiedName::parse("Inventory.Product").unwrap(),
            kind,
            catalog_version: CatalogVersion::new(7),
        }
    }

    fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
        ColumnDescriptor {
            name: name.to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal,
        }
    }

    fn transaction_policy() -> TransactionPolicy {
        TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        }
    }

    fn result_stream(columns: Vec<ColumnDescriptor>) -> ResultStreamContract {
        ResultStreamContract {
            name: "Reservation".to_string(),
            columns,
            row_count_exact_required: true,
        }
    }

    #[test]
    fn procedure_contract_requires_nonzero_hash_and_permission() {
        let contract = ProcedureContract {
            object: object(ObjectKind::Procedure),
            procedure_id: ProcedureId::new(99),
            contract_hash: ContractHash::zero(),
            inputs: vec![column("ProductId", 0)],
            structured_inputs: Vec::new(),
            result_streams: Vec::new(),
            required_permissions: vec!["ExecuteProcedure".to_string()],
            transaction_policy: transaction_policy(),
            compatibility_policy: CompatibilityPolicy::ExactHash,
        };

        assert_eq!(
            contract.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn procedure_contract_rejects_duplicate_input_and_result_names() {
        let duplicate_inputs = ProcedureContract {
            object: object(ObjectKind::Procedure),
            procedure_id: ProcedureId::new(99),
            contract_hash: ContractHash::test_vector(1),
            inputs: vec![column("ProductId", 0), column("ProductId", 1)],
            structured_inputs: Vec::new(),
            result_streams: vec![ResultStreamContract {
                name: "Reservation".to_string(),
                columns: vec![column("Reserved", 0)],
                row_count_exact_required: true,
            }],
            required_permissions: vec!["ExecuteProcedure".to_string()],
            transaction_policy: transaction_policy(),
            compatibility_policy: CompatibilityPolicy::ExactHash,
        };

        let error = duplicate_inputs.validate().unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Catalog);
        assert!(error.message().contains("unique"));

        let duplicate_results = ProcedureContract {
            inputs: vec![column("ProductId", 0)],
            result_streams: vec![
                ResultStreamContract {
                    name: "Reservation".to_string(),
                    columns: vec![column("Reserved", 0)],
                    row_count_exact_required: true,
                },
                ResultStreamContract {
                    name: "Reservation".to_string(),
                    columns: vec![column("ReservedAgain", 0)],
                    row_count_exact_required: true,
                },
            ],
            ..duplicate_inputs
        };

        let error = duplicate_results.validate().unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("result stream names"));
    }

    #[test]
    fn procedure_contract_rejects_duplicate_dependencies_and_permissions() {
        let duplicate_dependency = ProcedureContract {
            object: object(ObjectKind::Procedure),
            procedure_id: ProcedureId::new(99),
            contract_hash: ContractHash::test_vector(1),
            inputs: vec![column("ProductId", 0)],
            structured_inputs: vec![
                QualifiedName::parse("Inventory.StockRequest").unwrap(),
                QualifiedName::parse("Inventory.StockRequest").unwrap(),
            ],
            result_streams: vec![result_stream(vec![column("Reserved", 0)])],
            required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
            transaction_policy: transaction_policy(),
            compatibility_policy: CompatibilityPolicy::ExactHash,
        };

        let error = duplicate_dependency.validate().unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("structured input names"));

        let duplicate_permission = ProcedureContract {
            structured_inputs: Vec::new(),
            required_permissions: vec![
                "Inventory.ReserveStock.Execute".to_string(),
                "Inventory.ReserveStock.Execute".to_string(),
            ],
            ..duplicate_dependency
        };

        let error = duplicate_permission.validate().unwrap_err();
        assert_eq!(error.kind(), AndromedaErrorKind::Security);
        assert!(error.message().contains("permissions"));
    }

    #[test]
    fn procedure_contract_candidate_materializes_stable_canonical_hash() {
        let candidate = ProcedureContractCandidate {
            object: object(ObjectKind::Procedure),
            procedure_id: ProcedureId::new(99),
            inputs: vec![column("ProductId", 0)],
            structured_inputs: Vec::new(),
            result_streams: vec![result_stream(vec![column("Reserved", 0)])],
            required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
            transaction_policy: transaction_policy(),
            compatibility_policy: CompatibilityPolicy::ExactHash,
        };

        let first = candidate.clone().materialize().unwrap();
        let second = candidate.materialize().unwrap();

        assert_eq!(first.contract_hash, second.contract_hash);
        assert!(!first.contract_hash.is_zero());
        assert!(first.validate_canonical_hash().is_ok());

        let changed = ProcedureContractCandidate {
            result_streams: vec![result_stream(vec![
                column("Reserved", 0),
                column("Quantity", 1),
            ])],
            ..ProcedureContractCandidate {
                object: object(ObjectKind::Procedure),
                procedure_id: ProcedureId::new(99),
                inputs: vec![column("ProductId", 0)],
                structured_inputs: Vec::new(),
                result_streams: Vec::new(),
                required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
                transaction_policy: transaction_policy(),
                compatibility_policy: CompatibilityPolicy::ExactHash,
            }
        }
            .materialize()
            .unwrap();

        assert_ne!(first.contract_hash, changed.contract_hash);
    }

    #[test]
    fn compatibility_diagnostics_distinguish_exact_and_additive_changes() {
        let previous = ProcedureContractCandidate {
            object: object(ObjectKind::Procedure),
            procedure_id: ProcedureId::new(99),
            inputs: vec![column("ProductId", 0)],
            structured_inputs: Vec::new(),
            result_streams: vec![result_stream(vec![column("Reserved", 0)])],
            required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
            transaction_policy: transaction_policy(),
            compatibility_policy: CompatibilityPolicy::ExactHash,
        }
            .materialize()
            .unwrap();

        let additive = ProcedureContractCandidate {
            compatibility_policy: CompatibilityPolicy::AdditiveOnly,
            result_streams: vec![result_stream(vec![
                column("Reserved", 0),
                column("Quantity", 1),
            ])],
            ..ProcedureContractCandidate {
                object: object(ObjectKind::Procedure),
                procedure_id: ProcedureId::new(99),
                inputs: vec![column("ProductId", 0)],
                structured_inputs: Vec::new(),
                result_streams: Vec::new(),
                required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
                transaction_policy: transaction_policy(),
                compatibility_policy: CompatibilityPolicy::AdditiveOnly,
            }
        }
            .materialize()
            .unwrap();

        let exact_changed = ProcedureContract {
            compatibility_policy: CompatibilityPolicy::ExactHash,
            ..additive.clone()
        };

        assert!(additive.compatibility_with(&previous).compatible);
        let diagnostic = exact_changed.compatibility_with(&previous);
        assert!(!diagnostic.compatible);
        assert!(diagnostic.messages[0].contains("exact-hash"));
    }
}
