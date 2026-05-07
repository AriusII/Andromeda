use andromeda_error::AndromedaErrorKind;
use andromeda_proto::generated;

use super::common::{hash, valid_invocation_request};

#[test]
fn generated_invocation_request_requires_binding_identity_match() {
    let valid = valid_invocation_request();
    generated::validate_generated_invocation_request(&valid).unwrap();

    let mut wrong_hash = valid.clone();
    wrong_hash.correlation.as_mut().unwrap().contract_hash = Some(hash(0x22));
    let wrong_hash_error = generated::validate_generated_invocation_request(&wrong_hash)
        .expect_err("mismatched ContractHash must be rejected");
    assert_eq!(wrong_hash_error.kind(), AndromedaErrorKind::Contract);
    assert!(
        wrong_hash_error.message().contains("contract_hash"),
        "error should name the mismatched ContractHash field"
    );

    let mut wrong_catalog = valid.clone();
    wrong_catalog.correlation.as_mut().unwrap().catalog_version = Some(8);
    let wrong_catalog_error = generated::validate_generated_invocation_request(&wrong_catalog)
        .expect_err("mismatched CatalogVersion must be rejected");
    assert_eq!(wrong_catalog_error.kind(), AndromedaErrorKind::Contract);
    assert!(
        wrong_catalog_error.message().contains("catalog_version"),
        "error should name the mismatched CatalogVersion field"
    );

    let mut wrong_stats = valid.clone();
    wrong_stats.correlation.as_mut().unwrap().stats_version = Some(6);
    let wrong_stats_error = generated::validate_generated_invocation_request(&wrong_stats)
        .expect_err("mismatched StatsVersion must be rejected");
    assert_eq!(wrong_stats_error.kind(), AndromedaErrorKind::Contract);
    assert!(
        wrong_stats_error.message().contains("stats_version"),
        "error should name the mismatched StatsVersion field"
    );

    let mut missing_stats = valid;
    missing_stats.correlation.as_mut().unwrap().stats_version = None;
    assert!(generated::validate_generated_invocation_request(&missing_stats).is_err());
}

#[test]
fn generated_invocation_request_requires_expected_policy_version() {
    let valid = valid_invocation_request();
    generated::validate_generated_invocation_request(&valid).unwrap();

    let mut missing_policy = valid.clone();
    missing_policy
        .correlation
        .as_mut()
        .unwrap()
        .expected_policy_version = None;
    assert!(generated::validate_generated_invocation_request(&missing_policy).is_err());

    let mut zero_policy = valid;
    zero_policy
        .correlation
        .as_mut()
        .unwrap()
        .expected_policy_version = Some(0);
    assert!(generated::validate_generated_invocation_request(&zero_policy).is_err());
}
