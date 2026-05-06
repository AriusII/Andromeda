use andromeda_quic::{
    EarlyDataPolicy, ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy,
    ZeroRttAdmissionRejectionReason, ZeroRttReplayClass,
};

#[test]
fn zero_rtt_rejects_mutation_procedure() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    assert_eq!(
        policy.evaluate(ZeroRttReplayClass::MutatingProcedure),
        ZeroRttAdmissionDecision::Reject {
            class: ZeroRttReplayClass::MutatingProcedure,
            reason: ZeroRttAdmissionRejectionReason::MutatingProcedure,
        }
    );
}

#[test]
fn zero_rtt_default_rejects_mutating_procedure() {
    let policy = ZeroRttAdmissionPolicy::default();
    let decision = policy.evaluate(ZeroRttReplayClass::MutatingProcedure);

    assert_eq!(policy.early_data_policy(), EarlyDataPolicy::Disabled);
    assert!(!decision.is_admitted());
    assert_eq!(
        decision.rejection_reason(),
        Some(ZeroRttAdmissionRejectionReason::MutatingProcedure)
    );
}

#[test]
fn zero_rtt_rejects_unknown_idempotency() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    assert_eq!(
        policy.evaluate(ZeroRttReplayClass::UnknownIdempotency),
        ZeroRttAdmissionDecision::Reject {
            class: ZeroRttReplayClass::UnknownIdempotency,
            reason: ZeroRttAdmissionRejectionReason::UnknownIdempotency,
        }
    );
}

#[test]
fn zero_rtt_rejects_auth_changing_operation() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    assert_eq!(
        policy.evaluate(ZeroRttReplayClass::AuthChangingOperation),
        ZeroRttAdmissionDecision::Reject {
            class: ZeroRttReplayClass::AuthChangingOperation,
            reason: ZeroRttAdmissionRejectionReason::AuthChangingOperation,
        }
    );
}

#[test]
fn zero_rtt_rejects_catalog_procedure() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    assert_eq!(
        policy.evaluate(ZeroRttReplayClass::CatalogProcedure),
        ZeroRttAdmissionDecision::Reject {
            class: ZeroRttReplayClass::CatalogProcedure,
            reason: ZeroRttAdmissionRejectionReason::CatalogProcedure,
        }
    );
}

#[test]
fn zero_rtt_rejects_hadr_promotion_and_demotion() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    assert_eq!(
        policy.evaluate(ZeroRttReplayClass::HadrPromotion),
        ZeroRttAdmissionDecision::Reject {
            class: ZeroRttReplayClass::HadrPromotion,
            reason: ZeroRttAdmissionRejectionReason::HadrPromotion,
        }
    );
    assert_eq!(
        policy.evaluate(ZeroRttReplayClass::HadrDemotion),
        ZeroRttAdmissionDecision::Reject {
            class: ZeroRttReplayClass::HadrDemotion,
            reason: ZeroRttAdmissionRejectionReason::HadrDemotion,
        }
    );
}

#[test]
fn zero_rtt_read_only_manifest_is_classified_but_disabled_by_v1_doctrine() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();
    let decision = policy.evaluate(ZeroRttReplayClass::ReadOnlyManifest);

    assert_eq!(policy.early_data_policy(), EarlyDataPolicy::Disabled);
    assert!(ZeroRttReplayClass::ReadOnlyManifest.is_replay_safe());
    assert!(!decision.is_admitted());
    assert_eq!(decision.class(), ZeroRttReplayClass::ReadOnlyManifest);
    assert_eq!(
        decision.rejection_reason(),
        Some(ZeroRttAdmissionRejectionReason::DoctrineV1DisablesEarlyData)
    );
}
