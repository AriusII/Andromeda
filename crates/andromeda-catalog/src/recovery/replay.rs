use crate::{
    CatalogMutationRecord, CatalogSnapshot,
    recovery::{CatalogRecoveryAnomaly, CatalogRecoveryOutcome},
    wal_integration::{adapt_catalog_mutation_boundary, adapt_catalog_mutation_delta},
};

pub(super) type IndexedCatalogMutationRecord =
    andromeda_catalog_recovery::IndexedCatalogMutationRecord;

pub(super) fn replay_indexed_catalog_mutation_records(
    snapshot: CatalogSnapshot,
    records: Vec<IndexedCatalogMutationRecord>,
    anomalies: Vec<CatalogRecoveryAnomaly>,
) -> CatalogRecoveryOutcome {
    let outcome = andromeda_catalog_recovery::replay_indexed_catalog_mutation_records_into_target(
        snapshot, records, anomalies,
    );
    CatalogRecoveryOutcome {
        snapshot: outcome.target,
        report: outcome.report,
    }
}

pub(super) fn indexed_recovery_record(
    record_index: usize,
    record: CatalogMutationRecord,
) -> IndexedCatalogMutationRecord {
    let record = match record {
        CatalogMutationRecord::Begin(boundary) => {
            andromeda_catalog_recovery::CatalogMutationRecord::Begin(
                adapt_catalog_mutation_boundary(&boundary),
            )
        },
        CatalogMutationRecord::Apply(delta) => {
            andromeda_catalog_recovery::CatalogMutationRecord::Apply(Box::new(
                adapt_catalog_mutation_delta(*delta),
            ))
        },
        CatalogMutationRecord::Commit(boundary) => {
            andromeda_catalog_recovery::CatalogMutationRecord::Commit(
                adapt_catalog_mutation_boundary(&boundary),
            )
        },
    };
    IndexedCatalogMutationRecord {
        record_index,
        record,
    }
}
