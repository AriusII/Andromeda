use super::compatibility::{
    ALL_LOCK_MODES, EXPECTED_LOCK_MODE_COMPATIBILITY, expected_lock_mode_compatibility,
};

#[test]
fn lock_mode_v0_compatibility_matrix_is_exhaustive() {
    let mut observed_pairs = 0;
    for existing in ALL_LOCK_MODES {
        for requested in ALL_LOCK_MODES {
            assert_eq!(
                existing.is_compatible_with(requested),
                expected_lock_mode_compatibility(existing, requested),
                "existing={existing:?}, requested={requested:?}"
            );
            observed_pairs += 1;
        }
    }

    assert_eq!(observed_pairs, 36);
    assert_eq!(EXPECTED_LOCK_MODE_COMPATIBILITY.len(), 36);
}

#[test]
fn lock_mode_v0_compatibility_matrix_is_symmetric() {
    for existing in ALL_LOCK_MODES {
        for requested in ALL_LOCK_MODES {
            assert_eq!(
                existing.is_compatible_with(requested),
                requested.is_compatible_with(existing),
                "existing={existing:?}, requested={requested:?}"
            );
        }
    }
}
