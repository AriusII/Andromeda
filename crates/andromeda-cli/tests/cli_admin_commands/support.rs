use andromeda_cli::dispatch_command;
pub(crate) use andromeda_test_support::process::{
    assert_contains_all, assert_success, owned_args, stdout_lossy as stdout,
};
use andromeda_test_support::{process::run_binary, workspace::TestTempDir};
use std::{
    path::{Path, PathBuf},
    process::Output,
};

pub(crate) fn assert_dispatch_success<const N: usize>(args: [&str; N]) {
    let args = owned_args(args);
    assert_dispatch_success_owned(args);
}

pub(crate) fn assert_dispatch_success_owned(args: Vec<String>) {
    assert!(
        dispatch_command(&args).is_ok(),
        "expected dispatch success for {args:?}"
    );
}

pub(crate) fn assert_dispatch_error<const N: usize>(args: [&str; N]) {
    let args = owned_args(args);
    assert!(
        dispatch_command(&args).is_err(),
        "expected dispatch error for {args:?}"
    );
}

pub(crate) fn dispatch_error_message<const N: usize>(args: [&str; N]) -> String {
    let args = owned_args(args);
    dispatch_command(&args)
        .expect_err("expected dispatch error")
        .message()
        .to_string()
}

pub(crate) fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    run_binary(env!("CARGO_BIN_EXE_andromeda-cli"), args)
}

pub(crate) fn run_cli_vec(args: Vec<&str>) -> Output {
    run_binary(env!("CARGO_BIN_EXE_andromeda-cli"), args)
}

pub(crate) fn run_cli_owned(args: Vec<String>) -> Output {
    run_binary(env!("CARGO_BIN_EXE_andromeda-cli"), args)
}

pub(crate) fn assert_operator_boundary_json(text: &str, expected_schema: &str) {
    assert!(
        text.trim_start().starts_with('{'),
        "expected JSON output: {text}"
    );
    assert!(
        text.contains(&format!("\"schema\":\"{expected_schema}\"")),
        "expected schema `{expected_schema}` in `{text}`"
    );
    assert_no_application_procedure_surface(text);
    assert_no_ad_hoc_sql_surface(text);
}

pub(crate) fn assert_no_application_procedure_surface(text: &str) {
    for forbidden in [
        "\"surface\":\"application\"",
        "\"required_permission\":\"execute-procedure\"",
        "\"permission\":\"execute-procedure\"",
        "\"family\":\"procedure-invocation\"",
        "\"procedure_id\"",
        "\"procedure\"",
    ] {
        assert!(
            !text.contains(forbidden),
            "operator command output exposed application procedure surface token `{forbidden}` in `{text}`"
        );
    }
}

pub(crate) fn assert_no_ad_hoc_sql_surface(text: &str) {
    let lower = text.to_ascii_lowercase();
    for forbidden in [
        "\"sql\"",
        "\"query\"",
        "--sql",
        "select ",
        "insert ",
        "update ",
        "delete ",
        " from ",
        " where ",
    ] {
        assert!(
            !lower.contains(forbidden),
            "operator command output exposed ad hoc SQL token `{forbidden}` in `{text}`"
        );
    }
}

pub(crate) type TestArtifactDir = TestTempDir;

pub(crate) fn create_cli_backup_artifact(test_name: &str, backup_id: u64) -> TestArtifactDir {
    let artifact_dir = temp_artifact_dir(test_name);
    let output = run_cli_owned(vec![
        "backup".to_string(),
        "start".to_string(),
        "--runtime".to_string(),
        "--artifact-dir".to_string(),
        artifact_dir.display().to_string(),
        "--backup-id".to_string(),
        backup_id.to_string(),
        "--json".to_string(),
    ]);
    assert_success(&output);
    artifact_dir
}

pub(crate) fn temp_artifact_dir(test_name: &str) -> TestArtifactDir {
    TestTempDir::new(&format!("andromeda-cli-{test_name}"))
}

pub(crate) fn backup_manifest_path(root: &Path, backup_id: u64) -> PathBuf {
    root.join(format!("backup-{backup_id:016x}"))
        .join("backup.manifest")
}
