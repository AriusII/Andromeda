use andromeda_quic::ReconnectPolicy;
use proptest::prelude::*;

proptest! {
    #[test]
    fn reconnect_delay_is_deterministic_and_bounded(attempt in 1u32..=8u32) {
        let policy = ReconnectPolicy::conservative();
        let first = policy.delay_for_attempt(attempt).unwrap();
        let second = policy.delay_for_attempt(attempt).unwrap();
        let (min_delay, max_delay) = policy.jitter_bounds_for_attempt(attempt).unwrap();

        prop_assert_eq!(first, second);
        prop_assert!(first >= min_delay);
        prop_assert!(first <= max_delay);
    }
}
