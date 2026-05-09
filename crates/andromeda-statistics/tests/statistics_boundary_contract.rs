use andromeda_contract::StatsVersion;
use andromeda_statistics::{
    CorrelationEvidenceBounds, CorrelationStrengthPermille, StatsColumnTarget, StatsCorrelation,
    StatsCorrelationId, StatsCorrelationKind, StatsCorrelationPublicationBuilder,
};
use andromeda_types::{CatalogObjectId, CatalogVersion};

fn target(object_id: u64, column_index: u16) -> StatsColumnTarget {
    StatsColumnTarget::new(CatalogObjectId::new(object_id), column_index)
}

#[test]
fn correlation_publication_is_advisory_only() {
    let correlation = StatsCorrelation::new(
        StatsCorrelationId::new(7).unwrap(),
        CatalogVersion::new(11),
        StatsVersion::new(3),
        StatsCorrelationKind::JoinKeyEquivalence,
        CorrelationStrengthPermille::from_permille(900).unwrap(),
        vec![target(41, 0), target(41, 1)],
        CorrelationEvidenceBounds {
            sample_rows: 100,
            population_lower_bound: 100,
            population_upper_bound: 1_000,
            confidence_permille: 850,
        },
    )
    .unwrap();

    let publication = StatsCorrelationPublicationBuilder::new(CatalogVersion::new(11), StatsVersion::new(3))
        .unwrap()
        .push(correlation)
        .unwrap()
        .finish();

    assert!(!publication.is_authoritative());
    assert!(publication.requires_durable_publication_evidence());
    assert!(!publication.is_stale_for(CatalogVersion::new(11), StatsVersion::new(3)));
}
