//! Canonical hash computation for procedure contracts and policy versions.

use andromeda_digest::Sha256;
use andromeda_types::{
    ContractHash, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding, TimestampType,
};

use crate::names::QualifiedName;

use super::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, PolicyVersion,
    ProcedureContract, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    ResultStreamContract, StatsVersion, TransactionPolicy,
};

pub(super) fn canonical_procedure_contract_hash(contract: &ProcedureContract) -> ContractHash {
    canonical_procedure_contract_hash_parts(
        &contract.object.name,
        contract.stats_version,
        contract.protocol_layout,
        &contract.inputs,
        &contract.structured_inputs,
        &contract.result_streams,
        &contract.required_permissions,
        contract.transaction_policy,
        contract.compatibility_policy,
        contract.result_metadata_policy,
        &contract.error_policy,
        contract.multi_result_policy,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn canonical_procedure_contract_hash_parts(
    name: &QualifiedName,
    stats_version: StatsVersion,
    protocol_layout: ProtocolLayoutRef,
    inputs: &[andromeda_types::ColumnDescriptor],
    structured_inputs: &[QualifiedName],
    result_streams: &[ResultStreamContract],
    required_permissions: &[String],
    transaction_policy: TransactionPolicy,
    compatibility_policy: CompatibilityPolicy,
    result_metadata_policy: ResultMetadataPolicy,
    error_policy: &ProcedureErrorPolicy,
    multi_result_policy: MultiResultPolicy,
) -> ContractHash {
    let mut sink = StableHashSink::new();
    sink.str("andromeda.catalog.procedure-contract.v4.sha256");
    sink.qualified_name(name);
    sink.u64(stats_version.get());
    sink.contract_hash(protocol_layout.descriptor_set_hash);
    sink.contract_hash(protocol_layout.frame_envelope_hash);
    sink.columns(inputs);
    sink.u64(structured_inputs.len() as u64);
    for structured_input in structured_inputs {
        sink.qualified_name(structured_input);
    }
    sink.u64(result_streams.len() as u64);
    for stream in result_streams {
        sink.u64(stream.stream_id);
        sink.str(&stream.name);
        sink.u8(stream.cardinality.stable_tag());
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
    sink.u8(match result_metadata_policy {
        ResultMetadataPolicy::RequireBeforePayload => 0,
        ResultMetadataPolicy::AllowStreamingUnknown => 1,
    });
    sink.bool(error_policy.rollback_on_error);
    sink.u64(error_policy.allowed_error_codes.len() as u64);
    for code in &error_policy.allowed_error_codes {
        sink.str(code);
    }
    sink.u8(match multi_result_policy {
        MultiResultPolicy::SingleResultOnly => 0,
        MultiResultPolicy::MultipleResultStreamsAllowed => 1,
    });
    sink.finish()
}

pub(super) fn canonical_policy_version(
    stats_version: StatsVersion,
    required_permissions: &[String],
    transaction_policy: TransactionPolicy,
    compatibility_policy: CompatibilityPolicy,
    result_metadata_policy: ResultMetadataPolicy,
    error_policy: &ProcedureErrorPolicy,
    multi_result_policy: MultiResultPolicy,
) -> PolicyVersion {
    let mut sink = StableHashSink::new();
    sink.str("andromeda.catalog.policy-version.v1.sha256");
    sink.u64(stats_version.get());
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
    sink.u8(match result_metadata_policy {
        ResultMetadataPolicy::RequireBeforePayload => 0,
        ResultMetadataPolicy::AllowStreamingUnknown => 1,
    });
    sink.bool(error_policy.rollback_on_error);
    sink.u64(error_policy.allowed_error_codes.len() as u64);
    for code in &error_policy.allowed_error_codes {
        sink.str(code);
    }
    sink.u8(match multi_result_policy {
        MultiResultPolicy::SingleResultOnly => 0,
        MultiResultPolicy::MultipleResultStreamsAllowed => 1,
    });
    PolicyVersion::new(sink.finish().as_bytes())
}

struct StableHashSink {
    hasher: Sha256,
}

impl StableHashSink {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> ContractHash {
        ContractHash::new(self.hasher.finalize())
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update(&[value]);
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

    fn contract_hash(&mut self, value: ContractHash) {
        self.raw_bytes(&value.as_bytes());
    }

    fn qualified_name(&mut self, name: &QualifiedName) {
        self.u64(name.parts().len() as u64);
        for part in name.parts() {
            self.str(part);
        }
    }

    fn columns(&mut self, columns: &[andromeda_types::ColumnDescriptor]) {
        self.u64(columns.len() as u64);
        for column in columns {
            self.str(&column.name);
            self.type_descriptor(&column.data_type);
            self.u32(column.ordinal);
        }
    }

    fn type_descriptor(&mut self, descriptor: &andromeda_types::TypeDescriptor) {
        self.scalar_type(&descriptor.scalar);
        self.u8(match descriptor.absence {
            andromeda_types::AbsencePolicy::Required => 0,
            andromeda_types::AbsencePolicy::ExplicitOptional => 1,
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
