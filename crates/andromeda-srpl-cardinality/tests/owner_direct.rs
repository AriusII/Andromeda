use andromeda_contract::ResultStreamCardinality;
use andromeda_srpl_cardinality::Cardinality;

#[test]
fn owner_cardinality_enforces_public_row_bounds() {
    assert_eq!(Cardinality::One.min_row_count(), 1);
    assert_eq!(Cardinality::One.intrinsic_max_row_count(), Some(1));
    assert!(Cardinality::One.permits_exact_row_count(1));
    assert!(!Cardinality::One.permits_exact_row_count(0));
    assert!(!Cardinality::One.permits_exact_row_count(2));

    assert_eq!(Cardinality::Many.min_row_count(), 0);
    assert_eq!(Cardinality::Many.intrinsic_max_row_count(), None);
    assert!(Cardinality::Many.permits_row_count_max(10));
}

#[test]
fn owner_cardinality_round_trips_contract_cardinality() {
    let contract_cardinality: ResultStreamCardinality = Cardinality::NonEmptyMany.into();
    assert_eq!(
        Cardinality::from(contract_cardinality),
        Cardinality::NonEmptyMany
    );
}
