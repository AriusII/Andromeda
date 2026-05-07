use andromeda_catalog::{
    InvocationRuntimeRecord, InvocationRuntimeRecordOutcome, ProcedureContractBinding,
    ProcedureContractRef, ProcedureStore,
};
use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash, ProcedureId, TransactionId,
};
use andromeda_observe::DecisionTrace;
use andromeda_storage::{
    Lsn, PageId, PageSize, WalRecordKind, write_ahead_log::HeapRowRedoPayloadV1,
};

use crate::{InvocationCompletion, ResultStreamMetadata, WalDurabilityEvidence};

const LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V1: &[u8] =
    b"andromeda.exec.local.heap-row-insert-redo-template.v1";
const LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V2: &[u8] =
    b"andromeda.exec.local.heap-row-insert-redo-template.v2";
const LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_FIXED_LEN_V1: usize = 1 + 8 + 2 + 8 + 4;
const LOCAL_HEAP_ROW_REDO_BINDING_LEN: usize = 8 + 8 + 8 + ContractHash::LEN;
const LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_FIXED_LEN_V2: usize =
    1 + 8 + 2 + 8 + LOCAL_HEAP_ROW_REDO_BINDING_LEN + 4;

/// Catalog/procedure evidence carried next to local heap redo templates.
///
/// The HREDOV1 payload remains storage-generic and binary-compatible. This
/// binding prevents ProductStock redo from being accepted as anonymous heap
/// bytes by exec-side publication gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalHeapRowRedoContractBinding {
    pub table_object_id: CatalogObjectId,
    pub procedure_id: ProcedureId,
    pub catalog_version: CatalogVersion,
    pub contract_hash: ContractHash,
}

impl LocalHeapRowRedoContractBinding {
    pub fn new(
        table_object_id: CatalogObjectId,
        procedure_id: ProcedureId,
        catalog_version: CatalogVersion,
        contract_hash: ContractHash,
    ) -> AndromedaResult<Self> {
        let binding = Self {
            table_object_id,
            procedure_id,
            catalog_version,
            contract_hash,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn from_procedure_binding(
        table_object_id: CatalogObjectId,
        procedure: ProcedureContractBinding,
    ) -> AndromedaResult<Self> {
        Self::new(
            table_object_id,
            procedure.procedure_id,
            procedure.catalog_version,
            procedure.contract_hash,
        )
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.table_object_id.get() == 0 {
            return Err(local_redo_error(
                "local heap redo contract binding table object id must not be zero",
            ));
        }
        if self.procedure_id.get() == 0 {
            return Err(local_redo_error(
                "local heap redo contract binding procedure id must not be zero",
            ));
        }
        if self.catalog_version.get() == 0 {
            return Err(local_redo_error(
                "local heap redo contract binding catalog version must not be zero",
            ));
        }
        if self.contract_hash.is_zero() {
            return Err(local_redo_error(
                "local heap redo contract binding contract hash must not be zero",
            ));
        }
        Ok(())
    }
}

/// Local, pre-WAL template for a storage-owned heap row insert redo payload.
///
/// The template intentionally omits `resulting_page_lsn`: only the local WAL
/// dispatcher knows the mutation record LSN. At append time the dispatcher
/// materializes this into a HREDOV1 payload whose `resulting_page_lsn` must
/// match the WAL record LSN, making the record directly consumable by recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHeapRowInsertRedoTemplate {
    pub page_id: PageId,
    pub page_size: PageSize,
    pub slot_id: u16,
    pub expected_previous_page_lsn: Lsn,
    pub redo_binding: Option<LocalHeapRowRedoContractBinding>,
    pub tuple: Vec<u8>,
}

impl LocalHeapRowInsertRedoTemplate {
    pub fn new(
        page_id: PageId,
        page_size: PageSize,
        slot_id: u16,
        expected_previous_page_lsn: Lsn,
        tuple: Vec<u8>,
    ) -> AndromedaResult<Self> {
        let template = Self {
            page_id,
            page_size,
            slot_id,
            expected_previous_page_lsn,
            redo_binding: None,
            tuple,
        };
        template.validate()?;
        Ok(template)
    }

    pub fn new_with_contract_binding(
        page_id: PageId,
        page_size: PageSize,
        slot_id: u16,
        expected_previous_page_lsn: Lsn,
        redo_binding: LocalHeapRowRedoContractBinding,
        tuple: Vec<u8>,
    ) -> AndromedaResult<Self> {
        let template = Self {
            page_id,
            page_size,
            slot_id,
            expected_previous_page_lsn,
            redo_binding: Some(redo_binding),
            tuple,
        };
        template.validate()?;
        Ok(template)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.page_id.is_zero() {
            return Err(local_redo_error(
                "local heap redo template page id must not be zero",
            ));
        }

        if self.slot_id == u16::MAX {
            return Err(local_redo_error(
                "local heap row insert redo template slot id must be a real heap slot",
            ));
        }

        if self.tuple.is_empty() {
            return Err(local_redo_error(
                "local heap row insert redo template tuple must not be empty",
            ));
        }

        if self.tuple.len() > u32::MAX as usize {
            return Err(local_redo_error(
                "local heap row insert redo template tuple length exceeds u32",
            ));
        }

        if self.tuple.len() > self.page_size.bytes() as usize {
            return Err(local_redo_error(
                "local heap row insert redo template tuple length exceeds page size budget",
            ));
        }

        if let Some(binding) = self.redo_binding {
            binding.validate()?;
        }

        Ok(())
    }

    pub fn encode_template(&self) -> AndromedaResult<Vec<u8>> {
        self.validate()?;
        let domain = if self.redo_binding.is_some() {
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V2
        } else {
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V1
        };
        let fixed_len = if self.redo_binding.is_some() {
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_FIXED_LEN_V2
        } else {
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_FIXED_LEN_V1
        };
        let mut bytes = Vec::with_capacity(domain.len() + 1 + fixed_len + self.tuple.len());
        bytes.extend_from_slice(domain);
        bytes.push(0);
        bytes.push(page_size_tag(self.page_size));
        bytes.extend_from_slice(&self.page_id.get().to_le_bytes());
        bytes.extend_from_slice(&self.slot_id.to_le_bytes());
        bytes.extend_from_slice(&self.expected_previous_page_lsn.get().to_le_bytes());
        if let Some(binding) = self.redo_binding {
            bytes.extend_from_slice(&binding.table_object_id.get().to_le_bytes());
            bytes.extend_from_slice(&binding.procedure_id.get().to_le_bytes());
            bytes.extend_from_slice(&binding.catalog_version.get().to_le_bytes());
            bytes.extend_from_slice(&binding.contract_hash.as_bytes());
        }
        bytes.extend_from_slice(&(self.tuple.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.tuple);
        Ok(bytes)
    }

    pub fn try_decode_template(bytes: &[u8]) -> AndromedaResult<Option<Self>> {
        if bytes.starts_with(LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V2) {
            return Self::decode_template_with_domain(
                bytes,
                LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V2,
                true,
            )
            .map(Some);
        }

        if !bytes.starts_with(LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V1) {
            return Ok(None);
        }

        Self::decode_template_with_domain(
            bytes,
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_DOMAIN_V1,
            false,
        )
        .map(Some)
    }

    pub const fn redo_binding(&self) -> Option<LocalHeapRowRedoContractBinding> {
        self.redo_binding
    }

    fn decode_template_with_domain(
        bytes: &[u8],
        domain: &[u8],
        has_binding: bool,
    ) -> AndromedaResult<Self> {
        let separator_offset = domain.len();
        if bytes.get(separator_offset) != Some(&0) {
            return Err(local_redo_error(
                "local heap row insert redo template domain separator missing",
            ));
        }

        let data = &bytes[separator_offset + 1..];
        let fixed_len = if has_binding {
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_FIXED_LEN_V2
        } else {
            LOCAL_HEAP_ROW_INSERT_REDO_TEMPLATE_FIXED_LEN_V1
        };
        let fixed = data
            .get(..fixed_len)
            .ok_or_else(|| local_redo_error("local heap row insert redo template truncated"))?;

        let page_size = page_size_from_tag(fixed[0])?;
        let page_id = PageId::new(read_u64_le(fixed, 1, "page id")?);
        let slot_id = read_u16_le(fixed, 9, "slot id")?;
        let expected_previous_page_lsn =
            Lsn::new(read_u64_le(fixed, 11, "expected previous page LSN")?);
        let (redo_binding, tuple_len_offset) = if has_binding {
            let table_object_id = CatalogObjectId::new(read_u64_le(fixed, 19, "table object id")?);
            let procedure_id = ProcedureId::new(read_u64_le(fixed, 27, "procedure id")?);
            let catalog_version = CatalogVersion::new(read_u64_le(fixed, 35, "catalog version")?);
            let mut hash = [0_u8; ContractHash::LEN];
            hash.copy_from_slice(fixed.get(43..43 + ContractHash::LEN).ok_or_else(|| {
                local_redo_error("local heap row insert redo template contract hash truncated")
            })?);
            (
                Some(LocalHeapRowRedoContractBinding::new(
                    table_object_id,
                    procedure_id,
                    catalog_version,
                    ContractHash::new(hash),
                )?),
                43 + ContractHash::LEN,
            )
        } else {
            (None, 19)
        };
        let tuple_len = read_u32_le(fixed, tuple_len_offset, "tuple length")? as usize;
        let expected_len = fixed_len.checked_add(tuple_len).ok_or_else(|| {
            local_redo_error("local heap row insert redo template tuple length overflows")
        })?;
        if data.len() != expected_len {
            return Err(local_redo_error(format!(
                "local heap row insert redo template tuple length {tuple_len} does not match payload length {}",
                data.len()
            )));
        }

        let tuple = data[fixed_len..].to_vec();
        match redo_binding {
            Some(binding) => Self::new_with_contract_binding(
                page_id,
                page_size,
                slot_id,
                expected_previous_page_lsn,
                binding,
                tuple,
            ),
            None => Self::new(
                page_id,
                page_size,
                slot_id,
                expected_previous_page_lsn,
                tuple,
            ),
        }
    }

    pub fn materialize_heap_redo_payload(
        &self,
        resulting_page_lsn: Lsn,
    ) -> AndromedaResult<HeapRowRedoPayloadV1> {
        self.validate()?;
        HeapRowRedoPayloadV1::row_insert(
            self.page_id,
            self.page_size,
            self.slot_id,
            self.expected_previous_page_lsn,
            resulting_page_lsn,
            self.tuple.clone(),
        )
        .map_err(|error| local_redo_error(error.message().to_string()))
    }

    pub fn materialize_wal_payload(&self, resulting_page_lsn: Lsn) -> AndromedaResult<Vec<u8>> {
        self.materialize_heap_redo_payload(resulting_page_lsn)?
            .encode_for_wal_kind(WalRecordKind::RowInsert)
            .map_err(|error| local_redo_error(error.message().to_string()))
    }
}

fn page_size_tag(page_size: PageSize) -> u8 {
    match page_size {
        PageSize::KiB16 => 1,
        PageSize::KiB32 => 2,
    }
}

fn page_size_from_tag(tag: u8) -> AndromedaResult<PageSize> {
    match tag {
        1 => Ok(PageSize::KiB16),
        2 => Ok(PageSize::KiB32),
        _ => Err(local_redo_error(format!(
            "unknown local heap row insert redo template page size tag {tag}"
        ))),
    }
}

fn read_u16_le(bytes: &[u8], offset: usize, label: &str) -> AndromedaResult<u16> {
    let data = bytes.get(offset..offset + 2).ok_or_else(|| {
        local_redo_error(format!(
            "local heap row insert redo template truncated at {label}"
        ))
    })?;
    let mut buffer = [0_u8; 2];
    buffer.copy_from_slice(data);
    Ok(u16::from_le_bytes(buffer))
}

fn read_u32_le(bytes: &[u8], offset: usize, label: &str) -> AndromedaResult<u32> {
    let data = bytes.get(offset..offset + 4).ok_or_else(|| {
        local_redo_error(format!(
            "local heap row insert redo template truncated at {label}"
        ))
    })?;
    let mut buffer = [0_u8; 4];
    buffer.copy_from_slice(data);
    Ok(u32::from_le_bytes(buffer))
}

fn read_u64_le(bytes: &[u8], offset: usize, label: &str) -> AndromedaResult<u64> {
    let data = bytes.get(offset..offset + 8).ok_or_else(|| {
        local_redo_error(format!(
            "local heap row insert redo template truncated at {label}"
        ))
    })?;
    let mut buffer = [0_u8; 8];
    buffer.copy_from_slice(data);
    Ok(u64::from_le_bytes(buffer))
}

fn local_redo_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProcedure {
    pub contract: ProcedureContractRef,
    pub contract_binding: ProcedureContractBinding,
    pub required_permissions: Vec<String>,
    pub result_metadata: ResultStreamMetadata,
    pub mutation_payload: Vec<u8>,
    pub rows_affected: u64,
}

impl LocalProcedure {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract.validate()?;
        self.contract_binding.validate()?;
        if self.contract_binding.as_legacy_ref() != self.contract {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "local executable ProcedureContractBinding must match local Procedure contract",
            ));
        }
        self.result_metadata.validate_before_payload()?;

        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "required permission must not be empty",
                ));
            }
        }

        if self.mutation_payload.is_empty() && self.rows_affected != 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "mutation payload must exist when rows are affected",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerticalInvocationOutcome {
    pub completion: InvocationCompletion,
    /// Recovery-safe transaction id allocated by the runtime's
    /// `TransactionManager` for this invocation. This is the authoritative
    /// id stamped on every WAL record and result-stream frame; downstream
    /// emitters must read it from here rather than re-deriving it from
    /// [`InvocationCompletion::invocation_id`].
    pub transaction_id: TransactionId,
    pub admission_trace: DecisionTrace,
    pub contract_trace: DecisionTrace,
    pub authorization_trace: Option<DecisionTrace>,
    pub result_metadata: ResultStreamMetadata,
    pub runtime_record: InvocationRuntimeRecord,
    pub wal_evidence: Option<WalDurabilityEvidence>,
}

impl VerticalInvocationOutcome {
    pub const fn procedure_runtime_record(&self) -> &InvocationRuntimeRecord {
        &self.runtime_record
    }

    pub fn attach_runtime_to_procedure_store(
        &self,
        store: &mut ProcedureStore,
    ) -> AndromedaResult<InvocationRuntimeRecordOutcome> {
        store.attach_invocation_runtime(self.runtime_record.clone())
    }
}
