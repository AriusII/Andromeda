#![forbid(unsafe_code)]

use andromeda_test_support::{
    process::{assert_success, run_binary, run_binary_with_path, stdout_utf8 as stdout},
    workspace::unique_temp_path,
};
use std::fs;

#[test]
fn help_advertises_neutral_inventory_demo_names() {
    let output = run_binary(cli_binary(), ["--help"]);
    assert_success(&output);

    let help = stdout(&output);
    assert!(help.contains("andromeda-cli inventory-demo"));
    assert!(help.contains("andromeda-cli inventory-recoverable [--wal <path>]"));
    assert!(!help.contains("Phase 1"));
    assert!(!help.contains("vertical prototype"));
    assert!(!help.contains("vertical-v0 [--wal <path>]"));
}

#[test]
fn recoverable_command_alias_keeps_existing_vertical_v0_entrypoint() {
    let wal_path = unique_temp_path("andromeda-cli-vertical-v0-alias", ".wal");
    fs::remove_file(&wal_path).ok();

    let output = run_binary_with_path(cli_binary(), ["vertical-v0", "--wal"], &wal_path);
    assert_success(&output);

    let human = stdout(&output);
    assert!(human.contains("Andromeda recoverable inventory procedure demo"));
    assert!(human.contains("status: Committed"));

    fs::remove_file(&wal_path).ok();
}

fn cli_binary() -> &'static str {
    env!("CARGO_BIN_EXE_andromeda-cli")
}
