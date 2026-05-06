#![no_main]

use andromeda_quic::{
    EarlyDataPolicy, ZeroRttAdmissionPolicy, ZeroRttAdmissionRejectionReason, ZeroRttReplayClass,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let policy = if data.first().copied().unwrap_or_default() % 2 == 0 {
        ZeroRttAdmissionPolicy::doctrine_v1_disabled()
    } else {
        ZeroRttAdmissionPolicy::from_early_data_policy(EarlyDataPolicy::Disabled)
    };

    for byte in data.iter().copied().take(1024) {
        let class = replay_class(byte);
        let decision = policy.evaluate(class);

        assert_eq!(decision.class(), class);
        assert!(!decision.is_admitted());
        assert_eq!(class.is_replay_safe(), replay_safe_by_class(class));
        assert_eq!(decision.rejection_reason(), Some(expected_reason(class)));
    }
});

const fn replay_class(byte: u8) -> ZeroRttReplayClass {
    match byte % 8 {
        0 => ZeroRttReplayClass::MutatingProcedure,
        1 => ZeroRttReplayClass::UnknownIdempotency,
        2 => ZeroRttReplayClass::AuthChangingOperation,
        3 => ZeroRttReplayClass::CatalogProcedure,
        4 => ZeroRttReplayClass::HadrPromotion,
        5 => ZeroRttReplayClass::HadrDemotion,
        6 => ZeroRttReplayClass::ReadOnlyManifest,
        _ => ZeroRttReplayClass::ReadOnlyTelemetry,
    }
}

const fn expected_reason(class: ZeroRttReplayClass) -> ZeroRttAdmissionRejectionReason {
    match class {
        ZeroRttReplayClass::MutatingProcedure => ZeroRttAdmissionRejectionReason::MutatingProcedure,
        ZeroRttReplayClass::UnknownIdempotency => {
            ZeroRttAdmissionRejectionReason::UnknownIdempotency
        }
        ZeroRttReplayClass::AuthChangingOperation => {
            ZeroRttAdmissionRejectionReason::AuthChangingOperation
        }
        ZeroRttReplayClass::CatalogProcedure => ZeroRttAdmissionRejectionReason::CatalogProcedure,
        ZeroRttReplayClass::HadrPromotion => ZeroRttAdmissionRejectionReason::HadrPromotion,
        ZeroRttReplayClass::HadrDemotion => ZeroRttAdmissionRejectionReason::HadrDemotion,
        ZeroRttReplayClass::ReadOnlyManifest | ZeroRttReplayClass::ReadOnlyTelemetry => {
            ZeroRttAdmissionRejectionReason::DoctrineV1DisablesEarlyData
        }
    }
}

const fn replay_safe_by_class(class: ZeroRttReplayClass) -> bool {
    matches!(
        class,
        ZeroRttReplayClass::ReadOnlyManifest | ZeroRttReplayClass::ReadOnlyTelemetry
    )
}
