//! Canonical hash computation for procedure contracts and policy versions.

use andromeda_digest::Sha256;
use andromeda_types::ContractHash;

use crate::names::QualifiedName;
use crate::type_encoding::canonical_type_descriptor_bytes;

use super::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, PolicyVersion,
    ProcedureContract, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy,
    ResultStreamContract, StatsVersion, TransactionPolicy,
};

pub(super) fn canonical_procedure_contract_hash(contract: &ProcedureContract) -> ContractHash {
    canonical_procedure_contract_hash_parts(
        &contract.object.name,
        contract.protocol_layout,
        &contract.inputs,
        &contract.structured_inputs,
        &contract.result_streams,
        contract.stats_version,
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
    protocol_layout: ProtocolLayoutRef,
    inputs: &[andromeda_types::ColumnDescriptor],
    structured_inputs: &[QualifiedName],
    result_streams: &[ResultStreamContract],
    stats_version: StatsVersion,
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
    // Policy fields — must match canonical_policy_version field set so that any
    // policy change also changes the contract hash.
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
            self.raw_bytes(&canonical_type_descriptor_bytes(&column.data_type));
            self.u32(column.ordinal);
        }
    }
}
