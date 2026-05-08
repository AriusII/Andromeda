#![no_main]

use andromeda_security_contract::{
    ALL_PERMISSION_FAMILIES, ALL_SECURITY_ADMISSION_V0_BOUNDARIES,
    ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES, ALL_SECURITY_ADMISSION_V0_OUTCOMES,
    ALL_SECURITY_ADMISSION_V0_REASON_CODES, ALL_SECURITY_ADMISSION_V0_STEPS,
    SecurityAdmissionBoundaryV0, SecurityAdmissionEvidenceCodeV0, SecurityAdmissionOutcomeV0,
    SecurityAdmissionReasonCodeV0, SecurityAdmissionStepV0, SecurityAdmissionV0,
};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_ADMISSION_ITERATIONS: usize = 128;

fuzz_target!(|data: &[u8]| {
    assert_static_code_sets_roundtrip();

    let data = common::bounded_input(data, 4 * 1024);
    for chunk in data.chunks(6).take(MAX_ADMISSION_ITERATIONS) {
        let step = select(&ALL_SECURITY_ADMISSION_V0_STEPS, chunk.first().copied());
        let evidence = select(
            &ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES,
            chunk.get(1).copied(),
        );
        let outcome = select(&ALL_SECURITY_ADMISSION_V0_OUTCOMES, chunk.get(2).copied());
        let reason = select(
            &ALL_SECURITY_ADMISSION_V0_REASON_CODES,
            chunk.get(3).copied(),
        );
        let boundary = select(&ALL_SECURITY_ADMISSION_V0_BOUNDARIES, chunk.get(4).copied());
        let family = select(&ALL_PERMISSION_FAMILIES, chunk.get(5).copied());

        let admission = SecurityAdmissionV0::new(step, evidence, outcome, reason);
        if evidence.is_missing() {
            assert_eq!(admission.outcome(), SecurityAdmissionOutcomeV0::Denied);
            assert_eq!(
                admission.reason_code(),
                SecurityAdmissionReasonCodeV0::MissingEvidence
            );
            assert!(!admission.is_allowed());
        }

        assert_eq!(
            boundary.permits_family(family),
            boundary.surface().permits_family(family)
        );
        assert_eq!(
            SecurityAdmissionBoundaryV0::from_code(boundary.as_str()),
            Some(boundary)
        );
    }
});

fn select<T: Copy, const N: usize>(items: &[T; N], selector: Option<u8>) -> T {
    items[usize::from(selector.unwrap_or_default()) % N]
}

fn assert_static_code_sets_roundtrip() {
    for step in ALL_SECURITY_ADMISSION_V0_STEPS {
        assert_eq!(
            SecurityAdmissionStepV0::from_code(step.as_str()),
            Some(step)
        );
    }
    for evidence in ALL_SECURITY_ADMISSION_V0_EVIDENCE_CODES {
        assert_eq!(
            SecurityAdmissionEvidenceCodeV0::from_code(evidence.as_str()),
            Some(evidence)
        );
    }
    for outcome in ALL_SECURITY_ADMISSION_V0_OUTCOMES {
        assert_eq!(
            SecurityAdmissionOutcomeV0::from_code(outcome.as_str()),
            Some(outcome)
        );
    }
    for reason in ALL_SECURITY_ADMISSION_V0_REASON_CODES {
        assert_eq!(
            SecurityAdmissionReasonCodeV0::from_code(reason.as_str()),
            Some(reason)
        );
    }
}
