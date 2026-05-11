//! Wire-tag stability proof for [`PlanClass`].
//!
//! This test file is the *golden record* for every byte that flows over the
//! wire (or into a digest) when a `PlanClass` variant is serialised via
//! [`PlanClass::as_tag`].  Its purpose is to:
//!
//! 1. Prove that the 4 **legacy** tags (0x01–0x04) have not moved since their
//!    first publication.
//! 2. Prove that the 8 **P09 spec** tags (0x05–0x0C) are assigned exactly as
//!    specified by the P09 reconciliation (audit A4 GAP-3).
//! 3. Guard against future accidental reuse of any tag value.
//!
//! The assertions below are intentionally byte-literal so that a careless
//! renaming or enum-discriminant change in [`identity.rs`] surfaces here as a
//! **compile error** (missing variant) or a **test failure** (wrong byte),
//! never as a silent wire-protocol break.

use andromeda_plan_cache::PlanClass;

/// Golden wire-tag table.
///
/// Format: `(variant, expected_tag_byte)`.
const GOLDEN_TAGS: &[(PlanClass, u8)] = &[
    // ── Legacy / internal variants ────────────────────────────────────────────
    (PlanClass::Singleton, 0x01),
    (PlanClass::ParameterShape, 0x02),
    (PlanClass::Cardinality, 0x03),
    (PlanClass::StatsAdaptive, 0x04),
    // ── P09 spec variants (GAP-3 reconciliation) ──────────────────────────────
    (PlanClass::Generic, 0x05),
    (PlanClass::Small, 0x06),
    (PlanClass::Medium, 0x07),
    (PlanClass::Large, 0x08),
    (PlanClass::Skewed, 0x09),
    (PlanClass::StructuredObjectSmall, 0x0A),
    (PlanClass::StructuredObjectLarge, 0x0B),
    (PlanClass::Maintenance, 0x0C),
];

#[test]
fn plan_class_wire_tags_are_stable_and_cover_p09_spec() {
    // 1. Total variant coverage
    assert_eq!(
        GOLDEN_TAGS.len(),
        PlanClass::VARIANT_COUNT,
        "GOLDEN_TAGS must enumerate every PlanClass variant; update this table \
         whenever a new variant is added"
    );

    // 2. Byte-for-byte golden assertion
    for (variant, expected) in GOLDEN_TAGS {
        let actual = variant.as_tag();
        assert_eq!(
            actual, *expected,
            "PlanClass::{variant:?} wire tag drifted: expected 0x{expected:02X}, got 0x{actual:02X}. \
             Wire tags are frozen — do NOT reassign discriminants.",
        );
    }

    // 3. All tags are unique (no silent collision between legacy and spec)
    let mut tags: Vec<u8> = GOLDEN_TAGS.iter().map(|(_, t)| *t).collect();
    let original_len = tags.len();
    tags.sort_unstable();
    tags.dedup();
    assert_eq!(
        tags.len(),
        original_len,
        "Duplicate wire tags detected — every PlanClass variant must map to a \
         unique byte value"
    );

    // 4. Legacy tags occupy the range 0x01–0x04 (frozen range)
    let legacy = &GOLDEN_TAGS[..4];
    for (variant, tag) in legacy {
        assert!(
            (0x01..=0x04).contains(tag),
            "Legacy variant {variant:?} must keep a tag in 0x01–0x04, got 0x{tag:02X}"
        );
    }

    // 5. P09 spec tags occupy the range 0x05–0x0C (frozen range)
    let spec = &GOLDEN_TAGS[4..];
    for (variant, tag) in spec {
        assert!(
            (0x05..=0x0C).contains(tag),
            "P09 spec variant {variant:?} must have a tag in 0x05–0x0C, got 0x{tag:02X}"
        );
    }
}

/// Verify that [`PlanClass::as_tag`] agrees with the `#[repr(u8)]`
/// discriminant: casting the variant as `u8` must yield the same byte.
#[test]
fn plan_class_repr_u8_matches_as_tag() {
    for (variant, expected_tag) in GOLDEN_TAGS {
        // SAFETY: PlanClass is #[repr(u8)], so transmuting a copy to u8 is
        // sound.  We use a copy (not a reference) to avoid alignment issues.
        let discriminant = *variant as u8;
        assert_eq!(
            discriminant, *expected_tag,
            "PlanClass::{variant:?}: repr(u8) discriminant 0x{discriminant:02X} \
             differs from as_tag() 0x{expected_tag:02X}. Keep discriminants and \
             as_tag() in sync."
        );
    }
}
