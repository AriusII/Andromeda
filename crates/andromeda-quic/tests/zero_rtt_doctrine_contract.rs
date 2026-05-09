//! 0-RTT doctrine lock for V1.
//!
//! This test suite enforces:
//! - `EarlyDataPolicy` remains `Disabled`-only at compile time; and
//! - the runtime-free admission policy keeps all 0-RTT classes barred in V1.

use andromeda_quic::{
    EarlyDataPolicy, ZeroRttAdmissionDecision, ZeroRttAdmissionPolicy,
    ZeroRttAdmissionRejectionReason, ZeroRttReplayClass,
};

#[test]
fn early_data_policy_is_disabled_only_variant() {
    fn compile_lock(policy: EarlyDataPolicy) -> bool {
        match policy {
            EarlyDataPolicy::Disabled => true,
        }
    }
    assert!(compile_lock(EarlyDataPolicy::Disabled));
}

#[test]
fn zero_rtt_policy_rejects_mutating_unknown_and_replay_safe_classes() {
    let policy = ZeroRttAdmissionPolicy::doctrine_v1_disabled();

    let cases = [
        (
            ZeroRttReplayClass::MutatingProcedure,
            ZeroRttAdmissionRejectionReason::MutatingProcedure,
        ),
        (
            ZeroRttReplayClass::UnknownIdempotency,
            ZeroRttAdmissionRejectionReason::UnknownIdempotency,
        ),
        (
            ZeroRttReplayClass::ReadOnlyManifest,
            ZeroRttAdmissionRejectionReason::DoctrineV1DisablesEarlyData,
        ),
    ];

    for (class, reason) in cases {
        assert_eq!(
            policy.evaluate(class),
            ZeroRttAdmissionDecision::Reject { class, reason },
            "0-RTT admission must reject classified request {:?}",
            class
        );
    }
}

#[test]
fn zero_rtt_replay_safe_classes_are_classified_but_still_barred_in_v1() {
    assert!(ZeroRttReplayClass::ReadOnlyManifest.is_replay_safe());
    assert!(ZeroRttReplayClass::ReadOnlyTelemetry.is_replay_safe());
    assert!(!ZeroRttReplayClass::MutatingProcedure.is_replay_safe());

    let policy = ZeroRttAdmissionPolicy::from_early_data_policy(EarlyDataPolicy::Disabled);
    for class in [
        ZeroRttReplayClass::ReadOnlyManifest,
        ZeroRttReplayClass::ReadOnlyTelemetry,
    ] {
        assert!(!policy.evaluate(class).is_admitted());
    }
}
