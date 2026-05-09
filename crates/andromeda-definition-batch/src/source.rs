use andromeda_contract::{CatalogObjectRef, ObjectKind, QualifiedName};
use andromeda_digest::Sha256;
use andromeda_types::{CatalogVersion, DatabaseId, NamespaceId};

use crate::{
    DefinitionBatchId, DefinitionBatchSourceHash, DefinitionOperation, operation::object_kind_tag,
};

/// Computes a deterministic digest over the exact ordered DefinitionBatch source.
///
/// This hash intentionally includes operation order and object shape hashes. It
/// complements `DefinitionBatchDependencyGraphHash`, which canonicalizes the
/// dependency graph and therefore ignores source order when the graph itself is
/// equivalent.
pub fn compute_definition_batch_source_hash(
    batch_id: DefinitionBatchId,
    database_id: DatabaseId,
    namespace_id: NamespaceId,
    base_version: CatalogVersion,
    operations: &[DefinitionOperation],
) -> DefinitionBatchSourceHash {
    let mut sink = DefinitionBatchSourceHashSink::new();
    sink.str("andromeda.catalog.definition-batch.source.v1.sha256");
    sink.u64(batch_id.get());
    sink.u64(database_id.get());
    sink.u64(namespace_id.get());
    sink.u64(base_version.get());
    sink.u64(operations.len() as u64);
    for (operation_index, operation) in operations.iter().enumerate() {
        sink.u64(operation_index as u64);
        match operation {
            DefinitionOperation::Create(definition) => {
                sink.u8(0);
                sink.object_ref(definition.object_ref());
                sink.raw_bytes(&definition.shape_hash().as_bytes());
            },
            DefinitionOperation::Deprecate(target) => {
                sink.u8(1);
                sink.object_ref(&target.object);
            },
        }
    }
    sink.finish()
}

struct DefinitionBatchSourceHashSink {
    hasher: Sha256,
}

impl DefinitionBatchSourceHashSink {
    fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> DefinitionBatchSourceHash {
        DefinitionBatchSourceHash::new(self.hasher.finalize())
    }

    fn raw_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        self.raw_bytes(bytes);
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update(&[value]);
    }

    fn u64(&mut self, value: u64) {
        self.raw_bytes(&value.to_le_bytes());
    }

    fn object_ref(&mut self, object: &CatalogObjectRef) {
        self.u64(object.object_id.get());
        self.qualified_name(&object.name);
        self.object_kind(object.kind);
        self.u64(object.catalog_version.get());
    }

    fn qualified_name(&mut self, name: &QualifiedName) {
        self.u64(name.parts().len() as u64);
        for part in name.parts() {
            self.str(part);
        }
    }

    fn object_kind(&mut self, kind: ObjectKind) {
        self.u8(object_kind_tag(kind));
    }
}
