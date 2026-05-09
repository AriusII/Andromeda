use super::*;

pub(super) const ALL_LOCK_MODES: [LockMode; 6] = [
    LockMode::Shared,
    LockMode::Exclusive,
    LockMode::IntentShared,
    LockMode::IntentExclusive,
    LockMode::SchemaShared,
    LockMode::SchemaExclusive,
];

pub(super) type CompatibilityCase = ((LockMode, LockMode), bool);

pub(super) const EXPECTED_LOCK_MODE_COMPATIBILITY: [CompatibilityCase; 36] = [
    ((LockMode::Shared, LockMode::Shared), true),
    ((LockMode::Shared, LockMode::Exclusive), false),
    ((LockMode::Shared, LockMode::IntentShared), true),
    ((LockMode::Shared, LockMode::IntentExclusive), false),
    ((LockMode::Shared, LockMode::SchemaShared), true),
    ((LockMode::Shared, LockMode::SchemaExclusive), false),
    ((LockMode::Exclusive, LockMode::Shared), false),
    ((LockMode::Exclusive, LockMode::Exclusive), false),
    ((LockMode::Exclusive, LockMode::IntentShared), false),
    ((LockMode::Exclusive, LockMode::IntentExclusive), false),
    ((LockMode::Exclusive, LockMode::SchemaShared), false),
    ((LockMode::Exclusive, LockMode::SchemaExclusive), false),
    ((LockMode::IntentShared, LockMode::Shared), true),
    ((LockMode::IntentShared, LockMode::Exclusive), false),
    ((LockMode::IntentShared, LockMode::IntentShared), true),
    ((LockMode::IntentShared, LockMode::IntentExclusive), true),
    ((LockMode::IntentShared, LockMode::SchemaShared), true),
    ((LockMode::IntentShared, LockMode::SchemaExclusive), false),
    ((LockMode::IntentExclusive, LockMode::Shared), false),
    ((LockMode::IntentExclusive, LockMode::Exclusive), false),
    ((LockMode::IntentExclusive, LockMode::IntentShared), true),
    ((LockMode::IntentExclusive, LockMode::IntentExclusive), true),
    ((LockMode::IntentExclusive, LockMode::SchemaShared), true),
    (
        (LockMode::IntentExclusive, LockMode::SchemaExclusive),
        false,
    ),
    ((LockMode::SchemaShared, LockMode::Shared), true),
    ((LockMode::SchemaShared, LockMode::Exclusive), false),
    ((LockMode::SchemaShared, LockMode::IntentShared), true),
    ((LockMode::SchemaShared, LockMode::IntentExclusive), true),
    ((LockMode::SchemaShared, LockMode::SchemaShared), true),
    ((LockMode::SchemaShared, LockMode::SchemaExclusive), false),
    ((LockMode::SchemaExclusive, LockMode::Shared), false),
    ((LockMode::SchemaExclusive, LockMode::Exclusive), false),
    ((LockMode::SchemaExclusive, LockMode::IntentShared), false),
    (
        (LockMode::SchemaExclusive, LockMode::IntentExclusive),
        false,
    ),
    ((LockMode::SchemaExclusive, LockMode::SchemaShared), false),
    (
        (LockMode::SchemaExclusive, LockMode::SchemaExclusive),
        false,
    ),
];

pub(super) fn expected_lock_mode_compatibility(existing: LockMode, requested: LockMode) -> bool {
    EXPECTED_LOCK_MODE_COMPATIBILITY
        .iter()
        .find(|((expected_existing, expected_requested), _)| {
            *expected_existing == existing && *expected_requested == requested
        })
        .map(|(_, compatible)| *compatible)
        .expect("every lock-mode pair must be listed")
}
