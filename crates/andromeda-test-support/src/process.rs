use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};

/// Run a test binary with the supplied arguments.
pub fn run_binary<I, S>(binary: &str, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(binary);
    command.args(args).output().expect("run test binary")
}

/// Run a test binary with arguments followed by a filesystem path.
pub fn run_binary_with_path<I, S>(binary: &str, args: I, path: &Path) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(binary);
    command
        .args(args)
        .arg(path)
        .output()
        .expect("run test binary")
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout_lossy(output),
        stderr_lossy(output)
    );
}

pub fn stdout_lossy(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stdout_utf8(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf8")
}

pub fn stderr_lossy(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn stderr_utf8(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is utf8")
}

pub fn assert_contains_all(text: &str, expected: &[&str]) {
    for item in expected {
        assert!(text.contains(item), "expected `{item}` in `{text}`");
    }
}

pub fn owned_args<const N: usize>(args: [&str; N]) -> Vec<String> {
    args.into_iter().map(str::to_string).collect()
}
