use andromeda_cli::dispatch_command;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
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

fn owned_args<const N: usize>(args: [&str; N]) -> Vec<String> {
    args.into_iter().map(str::to_string).collect()
}

pub(crate) fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

pub(crate) fn run_cli_vec(args: Vec<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

pub(crate) fn run_cli_owned(args: Vec<String>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

pub(crate) fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub(crate) fn assert_contains_all(text: &str, expected: &[&str]) {
    for item in expected {
        assert!(text.contains(item), "expected `{item}` in `{text}`");
    }
}

pub(crate) struct TestArtifactDir {
    root: PathBuf,
}

impl TestArtifactDir {
    pub(crate) fn path(&self) -> &Path {
        &self.root
    }

    pub(crate) fn display(&self) -> std::path::Display<'_> {
        self.root.display()
    }
}

impl Drop for TestArtifactDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

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
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos();
    TestArtifactDir {
        root: std::env::temp_dir().join(format!(
            "andromeda-cli-{test_name}-{}-{nonce}",
            std::process::id()
        )),
    }
}

pub(crate) fn backup_manifest_path(root: &Path, backup_id: u64) -> PathBuf {
    root.join(format!("backup-{backup_id:016x}"))
        .join("backup.manifest")
}
