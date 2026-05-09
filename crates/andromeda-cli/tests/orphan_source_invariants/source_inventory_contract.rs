use std::collections::BTreeSet;

use crate::source_inventory::{
    collect_active_rust_sources, collect_direct_rs_files,
    compute_module_conflicts_from_virtual_crate, compute_orphans_from_virtual_crate,
    is_ignored_inventory_relative_path,
};
use crate::support::{path_set, virtual_files, workspace_relative_path, workspace_root};

const ALLOWED_PRE_EXISTING_ORPHANS: &[AllowedOrphanException] = &[
    AllowedOrphanException {
        path: "crates/andromeda-catalog/src/plan_cache/admission.rs",
        owner: "andromeda-catalog plan-cache extraction",
        exit_criteria: "Exit criteria: wire the plan-cache admission module through the catalog crate root or remove it.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-catalog/src/plan_cache/decision.rs",
        owner: "andromeda-catalog plan-cache extraction",
        exit_criteria: "Exit criteria: wire the plan-cache decision module through the catalog crate root or remove it.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-catalog/src/plan_cache/identity.rs",
        owner: "andromeda-catalog plan-cache extraction",
        exit_criteria: "Exit criteria: wire the plan-cache identity module through the catalog crate root or remove it.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-catalog/src/plan_cache/limits.rs",
        owner: "andromeda-catalog plan-cache extraction",
        exit_criteria: "Exit criteria: wire the plan-cache limits module through the catalog crate root or remove it.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-proto/src/generated_validation/manifest.rs",
        owner: "andromeda-proto generated-validation extraction",
        exit_criteria: "Exit criteria: regenerate or wire manifest validation through the proto crate root, then remove this orphan exception.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/gc/collector.rs",
        owner: "andromeda-storage WAL GC extraction",
        exit_criteria: "Exit criteria: wire the WAL GC collector module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/gc/context.rs",
        owner: "andromeda-storage WAL GC extraction",
        exit_criteria: "Exit criteria: wire the WAL GC context module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/gc/model.rs",
        owner: "andromeda-storage WAL GC extraction",
        exit_criteria: "Exit criteria: wire the WAL GC model module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/gc/scheduler.rs",
        owner: "andromeda-storage WAL GC extraction",
        exit_criteria: "Exit criteria: wire the WAL GC scheduler module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/segment_reclaimability/boundary.rs",
        owner: "andromeda-storage WAL segment-reclaimability extraction",
        exit_criteria: "Exit criteria: wire the segment-reclaimability boundary module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/segment_reclaimability/decision.rs",
        owner: "andromeda-storage WAL segment-reclaimability extraction",
        exit_criteria: "Exit criteria: wire the segment-reclaimability decision module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/segment_reclaimability/evidence.rs",
        owner: "andromeda-storage WAL segment-reclaimability extraction",
        exit_criteria: "Exit criteria: wire the segment-reclaimability evidence module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-storage/src/write_ahead_log/segment_reclaimability/policy.rs",
        owner: "andromeda-storage WAL segment-reclaimability extraction",
        exit_criteria: "Exit criteria: wire the segment-reclaimability policy module through storage or move it into the extracted WAL owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-wal/src/write_ahead_log/compaction/context.rs",
        owner: "andromeda-wal compaction extraction",
        exit_criteria: "Exit criteria: wire the WAL compaction context module through andromeda-wal or remove the extracted orphan file.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-wal/src/write_ahead_log/compaction/model.rs",
        owner: "andromeda-wal compaction extraction",
        exit_criteria: "Exit criteria: wire the WAL compaction model module through andromeda-wal or remove the extracted orphan file.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-wal/src/write_ahead_log/compaction/operations.rs",
        owner: "andromeda-wal compaction extraction",
        exit_criteria: "Exit criteria: wire the WAL compaction operations module through andromeda-wal or remove the extracted orphan file.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-wal/src/write_ahead_log/compaction/scheduler.rs",
        owner: "andromeda-wal compaction extraction",
        exit_criteria: "Exit criteria: wire the WAL compaction scheduler module through andromeda-wal or remove the extracted orphan file.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-wal/src/write_ahead_log/compaction/tests.rs",
        owner: "andromeda-wal compaction extraction",
        exit_criteria: "Exit criteria: wire the WAL compaction tests module through andromeda-wal or remove the extracted orphan file.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-tx/src/lock_manager/entry.rs",
        owner: "andromeda-tx lock-manager extraction",
        exit_criteria: "Exit criteria: wire the lock-manager entry module through tx or move it into an extracted locking owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-tx/src/lock_manager/evidence.rs",
        owner: "andromeda-tx lock-manager extraction",
        exit_criteria: "Exit criteria: wire the lock-manager evidence module through tx or move it into an extracted locking owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-tx/src/lock_manager/manager_core.rs",
        owner: "andromeda-tx lock-manager extraction",
        exit_criteria: "Exit criteria: wire the lock-manager core module through tx or move it into an extracted locking owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-tx/src/lock_manager/mode.rs",
        owner: "andromeda-tx lock-manager extraction",
        exit_criteria: "Exit criteria: wire the lock-manager mode module through tx or move it into an extracted locking owner crate.",
    },
    AllowedOrphanException {
        path: "crates/andromeda-tx/src/lock_manager/resource.rs",
        owner: "andromeda-tx lock-manager extraction",
        exit_criteria: "Exit criteria: wire the lock-manager resource module through tx or move it into an extracted locking owner crate.",
    },
];

struct AllowedOrphanException {
    path: &'static str,
    owner: &'static str,
    exit_criteria: &'static str,
}

#[test]
fn orphan_guard_finds_no_new_active_rust_orphans_or_module_conflicts() {
    let workspace = workspace_root();
    let report = collect_active_rust_sources(&workspace);
    let observed = report.orphans(&workspace);
    let allowed = allowed_pre_existing_orphan_paths();
    let new_orphans = observed.difference(&allowed).collect::<Vec<_>>();
    let stale_allowed_orphans = allowed.difference(&observed).collect::<Vec<_>>();

    assert!(
        new_orphans.is_empty()
            && stale_allowed_orphans.is_empty()
            && report.module_conflicts.is_empty(),
        "active Rust source inventory violations detected.\n\
         New orphan files must be wired through a Cargo root, mod/#[path], or include!, \
         or justified in ALLOWED_PRE_EXISTING_ORPHANS. Stale orphan exceptions must be removed \
         once their exit criteria are met. Module conflicts must remove either \
         foo.rs or foo/mod.rs for the same declared module.\n\
         \nNew orphans:\n  - {}\n\nStale orphan exceptions:\n  - {}\n\nModule file conflicts:\n  - {}",
        new_orphans
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n  - "),
        stale_allowed_orphans
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n  - "),
        report
            .module_conflicts
            .iter()
            .map(|conflict| conflict.as_str())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

#[test]
fn allowed_orphan_exceptions_are_named_and_bounded() {
    let mut seen_paths = BTreeSet::new();

    for exception in ALLOWED_PRE_EXISTING_ORPHANS {
        assert!(
            seen_paths.insert(exception.path),
            "allowed orphan exception {} must be listed only once",
            exception.path
        );
        assert!(
            !exception.owner.trim().is_empty(),
            "allowed orphan exception {} must name an owner",
            exception.path
        );
        assert!(
            exception.exit_criteria.starts_with("Exit criteria: ")
                && exception.exit_criteria.len() > "Exit criteria: ".len(),
            "allowed orphan exception {} owned by {} must name exit criteria",
            exception.path,
            exception.owner
        );
    }
}

fn allowed_pre_existing_orphan_paths() -> BTreeSet<String> {
    ALLOWED_PRE_EXISTING_ORPHANS
        .iter()
        .map(|exception| exception.path.to_owned())
        .collect()
}

#[test]
fn active_rust_inventory_accounts_for_build_scripts_fuzz_targets_and_exclusions() {
    let workspace = workspace_root();
    let report = collect_active_rust_sources(&workspace);
    let roots = report
        .roots
        .iter()
        .map(|path| workspace_relative_path(&workspace, path))
        .collect::<BTreeSet<_>>();

    assert!(
        roots.contains("crates/andromeda-proto/build.rs"),
        "active inventory must include crate build scripts"
    );
    for fuzz_target in collect_direct_rs_files(&workspace.join("fuzz").join("fuzz_targets")) {
        let fuzz_target = workspace_relative_path(&workspace, &fuzz_target);
        assert!(
            roots.contains(&fuzz_target),
            "active inventory must include fuzz target root {fuzz_target}"
        );
    }
    assert!(
        roots
            .iter()
            .all(|path| !is_ignored_inventory_relative_path(path)),
        "active inventory roots must exclude target, fuzz/target, and .claude/worktrees paths"
    );

    for ignored in [
        "target/generated.rs",
        "fuzz/target/generated.rs",
        ".claude/worktrees/wave/src/lib.rs",
        "crates/andromeda-cli/target/debug/build.rs",
    ] {
        assert!(
            is_ignored_inventory_relative_path(ignored),
            "inventory exclusion should ignore {ignored}"
        );
    }
}

#[test]
fn orphan_guard_detects_synthetic_orphan_in_fixture() {
    let files = virtual_files(&[
        (
            "src/lib.rs",
            "mod a;\npub mod b;\npub mod inline { pub fn x() {} }\n",
        ),
        ("src/a.rs", ""),
        ("src/b/mod.rs", "pub mod c;\n"),
        ("src/b/c.rs", ""),
        ("src/orphan.rs", ""),
        ("src/b/orphan_sub.rs", ""),
    ]);
    let expected = path_set(&["src/b/orphan_sub.rs", "src/orphan.rs"]);

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

#[test]
fn orphan_guard_inline_mod_block_does_not_consume_a_file() {
    let files = virtual_files(&[
        ("src/lib.rs", "pub mod foo { pub fn x() {} }\n"),
        ("src/foo.rs", ""),
    ]);
    let expected = path_set(&["src/foo.rs"]);

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

#[test]
fn orphan_guard_file_module_reaches_sibling_directory_modules() {
    let files = virtual_files(&[
        ("src/lib.rs", "mod file;\n"),
        ("src/file.rs", "mod child;\n"),
        ("src/file/child.rs", ""),
        ("src/file/orphan.rs", ""),
    ]);
    let expected = path_set(&["src/file/orphan.rs"]);

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

#[test]
fn orphan_guard_target_root_reaches_sibling_support_module() {
    let files = virtual_files(&[
        ("tests/key_contract.rs", "mod support;\n"),
        ("tests/support/mod.rs", ""),
        ("tests/key_contract/orphan.rs", ""),
    ]);
    let expected = path_set(&["tests/key_contract/orphan.rs"]);

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}

#[test]
fn orphan_guard_include_macro_counts_literal_rust_source() {
    let files = virtual_files(&[
        ("src/lib.rs", "include!(\"generated/included.rs\");\n"),
        ("src/generated/included.rs", ""),
    ]);

    assert!(compute_orphans_from_virtual_crate(&files).is_empty());
}

#[test]
fn orphan_guard_detects_synthetic_file_vs_mod_rs_conflict() {
    let files = virtual_files(&[
        ("src/lib.rs", "mod optimizer;\n"),
        ("src/optimizer.rs", ""),
        ("src/optimizer/mod.rs", ""),
    ]);
    let expected = path_set(&[
        "src/lib.rs: mod optimizer resolves to both src/optimizer.rs and src/optimizer/mod.rs",
    ]);

    assert_eq!(
        compute_module_conflicts_from_virtual_crate(&files),
        expected
    );
}

#[test]
fn orphan_guard_path_attribute_resolves_from_declaring_file_directory() {
    let files = virtual_files(&[
        ("src/lib.rs", "mod manager;\n"),
        ("src/manager.rs", "mod manager_core;\n"),
        (
            "src/manager/manager_core.rs",
            "#[cfg(test)]\n#[path = \"tests.rs\"]\nmod tests;\n",
        ),
        ("src/manager/tests.rs", ""),
        ("src/manager/manager_core/tests.rs", ""),
    ]);
    let expected = path_set(&["src/manager/manager_core/tests.rs"]);

    assert_eq!(compute_orphans_from_virtual_crate(&files), expected);
}
