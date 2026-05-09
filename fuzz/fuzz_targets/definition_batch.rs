#![no_main]

use andromeda_catalog::CatalogMutationRecord;
use andromeda_catalog_store::{
    CatalogDefinition, CatalogObjectRef, EnumDefinition, EnumVariant, ObjectKind, QualifiedName,
    StructuredObjectDefinition, TableDefinition,
};
use andromeda_core::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, DatabaseId, NamespaceId, ScalarType,
    TypeDescriptor,
};
use andromeda_definition_batch::{
    CatalogLifecycleTarget, DefinitionBatch, DefinitionBatchId, DefinitionOperation,
};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_DEFINITION_BATCH_INPUT_BYTES: usize = 4096;
const MAX_OPERATIONS: usize = 16;
const MAX_COLUMNS: usize = 8;
const MAX_ENUM_VARIANTS: usize = 8;

fuzz_target!(|data: &[u8]| {
    let data = common::bounded_input(data, MAX_DEFINITION_BATCH_INPUT_BYTES);

    if let Ok(record) = CatalogMutationRecord::decode_durable_payload(data) {
        assert!(record.validate_for_durable_payload().is_ok());
        if let Ok(encoded) = record.encode_durable_payload() {
            assert_eq!(
                CatalogMutationRecord::decode_durable_payload(&encoded),
                Ok(record)
            );
        }
    }

    let batch = definition_batch_from_bytes(data);

    let source_hash = batch.source_hash();
    assert_eq!(source_hash, batch.source_hash());

    match (batch.dependency_graph_hash(), batch.dependency_graph_hash()) {
        (Ok(first), Ok(second)) => assert_eq!(first, second),
        (Err(_), Err(_)) => {},
        _ => {
            panic!(
                "DefinitionBatch dependency_graph_hash must be deterministic for identical input"
            );
        },
    }

    let first_plan = batch.dry_run();
    let second_plan = batch.dry_run();
    match (first_plan, second_plan) {
        (Ok(first), Ok(second)) => {
            assert_eq!(first, second);
            assert_eq!(first.batch_id, batch.batch_id);
            assert_eq!(first.database_id, batch.database_id);
            assert_eq!(first.namespace_id, batch.namespace_id);
            assert_eq!(first.operation_count, batch.operations.len());
            assert_eq!(first.previous_version, batch.base_version);
            assert_eq!(
                first.next_version.get(),
                batch.base_version.get().saturating_add(1)
            );
            assert_eq!(
                first.mutation_plan.record_count(),
                batch.operations.len().saturating_add(2)
            );
        },
        (Err(_), Err(_)) => {},
        _ => {
            panic!("DefinitionBatch dry_run must be deterministic for identical input");
        },
    }
});

fn definition_batch_from_bytes(data: &[u8]) -> DefinitionBatch {
    let mut cursor = Cursor::new(data);
    let batch_id = nonzero_u64(cursor.next_u64(), 0xD0);
    let database_id = nonzero_u64(cursor.next_u64(), 1);
    let namespace_id = nonzero_u64(cursor.next_u64(), 1);
    let base_version = (cursor.next_u64() % 31) + 1;
    let next_version = base_version + 1;
    let operation_count = usize::from(cursor.next_u8()) % (MAX_OPERATIONS + 1);

    let mut operations = Vec::with_capacity(operation_count);
    for operation_index in 0..operation_count {
        let Some(operation) =
            definition_operation(&mut cursor, operation_index, base_version, next_version)
        else {
            continue;
        };
        operations.push(operation);
    }

    DefinitionBatch {
        batch_id: DefinitionBatchId::new(batch_id),
        database_id: DatabaseId::new(database_id),
        namespace_id: NamespaceId::new(namespace_id),
        base_version: CatalogVersion::new(base_version),
        operations,
    }
}

fn definition_operation(
    cursor: &mut Cursor<'_>,
    operation_index: usize,
    base_version: u64,
    next_version: u64,
) -> Option<DefinitionOperation> {
    Some(match cursor.next_u8() % 4 {
        0 => DefinitionOperation::Create(table_definition(cursor, operation_index, next_version)?),
        1 => DefinitionOperation::Create(structured_definition(
            cursor,
            operation_index,
            next_version,
        )?),
        2 => DefinitionOperation::Create(enum_definition(cursor, operation_index, next_version)?),
        _ => {
            let catalog_version = lifecycle_target_version(cursor, base_version);
            DefinitionOperation::Deprecate(CatalogLifecycleTarget {
                object: object_ref(
                    cursor,
                    operation_index,
                    ObjectKind::Procedure,
                    catalog_version,
                )?,
            })
        },
    })
}

fn table_definition(
    cursor: &mut Cursor<'_>,
    operation_index: usize,
    next_version: u64,
) -> Option<CatalogDefinition> {
    let catalog_version = object_version(cursor, next_version);
    Some(CatalogDefinition::Table(TableDefinition {
        object: object_ref(cursor, operation_index, ObjectKind::Table, catalog_version)?,
        columns: columns(cursor),
    }))
}

fn structured_definition(
    cursor: &mut Cursor<'_>,
    operation_index: usize,
    next_version: u64,
) -> Option<CatalogDefinition> {
    let fields = columns(cursor);
    let unique_by = if cursor.next_u8() % 2 == 0 {
        fields
            .first()
            .map(|field| field.name.clone())
            .into_iter()
            .collect()
    } else {
        vec!["missing_unique_field".to_string()]
    };
    let catalog_version = object_version(cursor, next_version);

    Some(CatalogDefinition::StructuredObject(
        StructuredObjectDefinition {
            object: object_ref(
                cursor,
                operation_index,
                ObjectKind::StructuredObject,
                catalog_version,
            )?,
            fields,
            unique_by,
        },
    ))
}

fn enum_definition(
    cursor: &mut Cursor<'_>,
    operation_index: usize,
    next_version: u64,
) -> Option<CatalogDefinition> {
    let variant_count = usize::from(cursor.next_u8()) % (MAX_ENUM_VARIANTS + 1);
    let duplicate_values = cursor.next_u8() % 5 == 0;
    let catalog_version = object_version(cursor, next_version);
    let variants = (0..variant_count)
        .map(|index| EnumVariant {
            name: format!("V{index}"),
            value: if duplicate_values {
                1
            } else {
                index as i64 + 1
            },
        })
        .collect();

    Some(CatalogDefinition::Enum(EnumDefinition {
        object: object_ref(cursor, operation_index, ObjectKind::Enum, catalog_version)?,
        variants,
    }))
}

fn object_ref(
    cursor: &mut Cursor<'_>,
    operation_index: usize,
    kind: ObjectKind,
    catalog_version: u64,
) -> Option<CatalogObjectRef> {
    Some(CatalogObjectRef {
        object_id: CatalogObjectId::new(object_id(cursor, operation_index)),
        name: object_name(cursor, operation_index, kind)?,
        kind,
        catalog_version: CatalogVersion::new(catalog_version),
    })
}

fn object_name(
    cursor: &mut Cursor<'_>,
    operation_index: usize,
    kind: ObjectKind,
) -> Option<QualifiedName> {
    let duplicate_name_bucket = cursor.next_u8() % 8 == 0;
    let suffix = if duplicate_name_bucket {
        0
    } else {
        operation_index as u64 + 1
    };
    let kind_name = match kind {
        ObjectKind::Table => "Table",
        ObjectKind::StructuredObject => "Structured",
        ObjectKind::Enum => "Enum",
        ObjectKind::Procedure => "Procedure",
        _ => "Object",
    };

    QualifiedName::new(["Fuzz".to_string(), format!("{kind_name}{suffix}")]).ok()
}

fn columns(cursor: &mut Cursor<'_>) -> Vec<ColumnDescriptor> {
    let column_count = usize::from(cursor.next_u8()) % (MAX_COLUMNS + 1);
    let duplicate_names = cursor.next_u8() % 7 == 0;
    let sparse_ordinals = cursor.next_u8() % 5 == 0;

    (0..column_count)
        .map(|index| ColumnDescriptor {
            name: if duplicate_names {
                "c_dup".to_string()
            } else {
                format!("c{index}")
            },
            data_type: TypeDescriptor::required(scalar_from(cursor.next_u8())),
            ordinal: if sparse_ordinals {
                index.saturating_add(1) as u32
            } else {
                index as u32
            },
        })
        .collect()
}

fn scalar_from(selector: u8) -> ScalarType {
    match selector % 5 {
        0 => ScalarType::I64,
        1 => ScalarType::U64,
        2 => ScalarType::Bool,
        3 => ScalarType::I32,
        _ => ScalarType::U32,
    }
}

fn object_version(cursor: &mut Cursor<'_>, next_version: u64) -> u64 {
    if cursor.next_u8() % 6 == 0 {
        next_version.saturating_add(1)
    } else {
        next_version
    }
}

fn lifecycle_target_version(cursor: &mut Cursor<'_>, base_version: u64) -> u64 {
    if cursor.next_u8() % 4 == 0 {
        base_version.saturating_add(1)
    } else {
        base_version
    }
}

fn object_id(cursor: &mut Cursor<'_>, operation_index: usize) -> u64 {
    if cursor.next_u8() % 8 == 0 {
        1
    } else {
        operation_index as u64 + 1
    }
}

fn nonzero_u64(value: u64, fallback: u64) -> u64 {
    if value == 0 { fallback } else { value }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn next_u8(&mut self) -> u8 {
        let value = self.bytes.get(self.offset).copied().unwrap_or_default();
        self.offset = self.offset.saturating_add(1);
        value
    }

    fn next_u64(&mut self) -> u64 {
        let mut array = [0; 8];
        for byte in &mut array {
            *byte = self.next_u8();
        }
        u64::from_le_bytes(array)
    }
}
