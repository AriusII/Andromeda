//! Gate tests for [`block_application_surface`] and
//! [`application_surface_disposition`].
//!
//! P13 exit criterion line 74: *"ForensicStart produit rapport et bloque trafic
//! applicatif."*
//!
//! These tests confirm that the C5 fence is implemented at type level:
//! - [`StartupMode::ForensicStart`] always blocks the Application Surface.
//! - [`StartupMode::FastStart`] and [`StartupMode::SafeStart`] allow it.
//!
//! # Integration note
//!
//! The gate function exists at type level as of P13/W3.  Integration into the
//! QUIC bootstrap listener (`andromeda-quic`) is a P14+ task.

use andromeda_recovery::{
    ApplicationSurfaceDisposition, StartupMode, application_surface_disposition,
    block_application_surface,
};

// ── block_application_surface ─────────────────────────────────────────────────

#[test]
fn forensic_start_blocks_application_surface() {
    assert!(
        block_application_surface(StartupMode::ForensicStart),
        "ForensicStart MUST block the Application Surface (P13 exit criterion line 74)"
    );
}

#[test]
fn fast_start_allows_application_surface() {
    assert!(
        !block_application_surface(StartupMode::FastStart),
        "FastStart must NOT block the Application Surface"
    );
}

#[test]
fn safe_start_allows_application_surface() {
    assert!(
        !block_application_surface(StartupMode::SafeStart),
        "SafeStart must NOT block the Application Surface"
    );
}

// ── application_surface_disposition ──────────────────────────────────────────

#[test]
fn forensic_start_disposition_is_block_forensic() {
    assert_eq!(
        application_surface_disposition(StartupMode::ForensicStart),
        ApplicationSurfaceDisposition::BlockForensic,
        "ForensicStart disposition must be BlockForensic"
    );
}

#[test]
fn fast_start_disposition_is_allow() {
    assert_eq!(
        application_surface_disposition(StartupMode::FastStart),
        ApplicationSurfaceDisposition::Allow,
        "FastStart disposition must be Allow"
    );
}

#[test]
fn safe_start_disposition_is_allow() {
    assert_eq!(
        application_surface_disposition(StartupMode::SafeStart),
        ApplicationSurfaceDisposition::Allow,
        "SafeStart disposition must be Allow"
    );
}

// ── Consistency: bool and enum are coherent ───────────────────────────────────

#[test]
fn block_and_disposition_are_consistent_for_all_modes() {
    for mode in [
        StartupMode::FastStart,
        StartupMode::SafeStart,
        StartupMode::ForensicStart,
    ] {
        let blocks = block_application_surface(mode);
        let disposition = application_surface_disposition(mode);

        match disposition {
            ApplicationSurfaceDisposition::BlockForensic => {
                assert!(
                    blocks,
                    "disposition=BlockForensic but block_application_surface returned false for {mode:?}"
                );
            },
            ApplicationSurfaceDisposition::Allow => {
                assert!(
                    !blocks,
                    "disposition=Allow but block_application_surface returned true for {mode:?}"
                );
            },
        }
    }
}
