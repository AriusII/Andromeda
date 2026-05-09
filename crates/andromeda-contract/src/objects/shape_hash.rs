use andromeda_types::ContractHash;

use andromeda_digest::Sha256;
use andromeda_procedure_contract::{CatalogObjectRef, ObjectKind, ProcedureContract};
use andromeda_structured_object::{
    encode_column_descriptors_shape_material, encode_structured_object_shape_material,
};

use super::{EnumDefinition, StructuredObjectDefinition, TableDefinition};

pub(super) fn table_shape_hash(definition: &TableDefinition) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.table-shape.v2.sha256");
    sink.object_ref(&definition.object);
    sink.raw_bytes(&encode_column_descriptors_shape_material(
        &definition.columns,
    ));
    sink.finish()
}

pub(super) fn structured_object_shape_hash(
    definition: &StructuredObjectDefinition,
) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.structured-object-shape.v2.sha256");
    sink.object_ref(&definition.object);
    sink.raw_bytes(&encode_structured_object_shape_material(
        &definition.fields,
        &definition.unique_by,
    ));
    sink.finish()
}

pub(super) fn enum_shape_hash(definition: &EnumDefinition) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.enum-shape.v2.sha256");
    sink.object_ref(&definition.object);
    sink.u64(definition.variants.len() as u64);
    for variant in &definition.variants {
        sink.str(&variant.name);
        sink.raw_bytes(&variant.value.to_le_bytes());
    }
    sink.finish()
}

pub(super) fn procedure_shape_hash(definition: &ProcedureContract) -> ContractHash {
    let mut sink = ObjectShapeHashSink::new();
    sink.str("andromeda.catalog.procedure-definition-shape.v1.sha256");
    sink.object_ref(&definition.object);
    sink.u64(definition.procedure_id.get());
    sink.raw_bytes(&definition.contract_hash.as_bytes());
    sink.u64(definition.stats_version.get());
    sink.raw_bytes(&definition.policy_version().as_bytes());
    sink.finish()
}

struct ObjectShapeHashSink {
    hasher: Sha256,
}

impl ObjectShapeHashSink {
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

    fn u64(&mut self, value: u64) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn object_ref(&mut self, object: &CatalogObjectRef) {
        self.u64(object.object_id.get());
        self.u64(object.catalog_version.get());
        self.u8(match object.kind {
            ObjectKind::Database => 0,
            ObjectKind::Namespace => 1,
            ObjectKind::Table => 2,
            ObjectKind::Map => 3,
            ObjectKind::Enum => 4,
            ObjectKind::StructuredObject => 5,
            ObjectKind::Procedure => 6,
        });
        self.u64(object.name.parts().len() as u64);
        for part in object.name.parts() {
            self.str(part);
        }
    }
}
