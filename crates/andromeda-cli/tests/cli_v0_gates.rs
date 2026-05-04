#![forbid(unsafe_code)]

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn protocol_smoke_detail_output_is_deterministic() {
    let first = run_cli(["protocol-smoke", "--detail"]);
    let second = run_cli(["protocol-smoke", "--detail"]);
    assert_success(&first);
    assert_success(&second);

    let first_stdout = stdout(&first);
    let second_stdout = stdout(&second);

    assert_eq!(first_stdout, second_stdout);
    assert!(first_stdout.contains("payload/frame lockstep: ok (9 codes)"));
    assert!(first_stdout.contains("payload/frame codes:"));
    assert!(first_stdout.contains("representative result frames:"));
    assert!(first_stdout.contains("network sockets: not opened"));
}

#[test]
fn vertical_v0_writes_file_wal_and_recovery_inspect_reports_replay() {
    let wal_path = unique_temp_wal_path("andromeda-cli-v0");
    fs::remove_file(&wal_path).ok();

    let vertical = run_cli_with_path(["vertical-v0", "--wal"], &wal_path);
    assert_success(&vertical);

    let vertical_stdout = stdout(&vertical);
    assert!(vertical_stdout.contains("Andromeda V0 recoverable vertical prototype"));
    assert!(vertical_stdout.contains("status: Committed"));
    assert!(vertical_stdout.contains("rows affected: Some(2)"));
    assert!(vertical_stdout.contains("recovery replay LSNs: [2]"));
    assert!(vertical_stdout.contains("recovery boundary: Clean"));
    assert!(vertical_stdout.contains("forensic required: false"));
    assert!(vertical_stdout.contains("result frames: 3"));

    let inspect = run_cli_with_path(["recovery-inspect"], &wal_path);
    assert_success(&inspect);

    let inspect_stdout = stdout(&inspect);
    assert!(inspect_stdout.contains("Andromeda V0 WAL recovery inspect"));
    assert!(inspect_stdout.contains("startup mode: SafeStart"));
    assert!(inspect_stdout.contains("byte order: little-endian"));
    assert!(inspect_stdout.contains("durable prefix records: 3"));
    assert!(inspect_stdout.contains("boundary: Clean"));
    assert!(inspect_stdout.contains("scan stop: none"));
    assert!(inspect_stdout.contains("forensic required: false"));
    assert!(inspect_stdout.contains("replay LSNs: [2]"));

    fs::remove_file(&wal_path).ok();
}

#[test]
fn workspace_policy_gates_do_not_drift_through_cli_scope() {
    let root = workspace_root();
    let crate_sources = root.join("crates");
    let proto_schemas = root.join("schemas").join("proto");

    let manifest_files = collect_files(&root, |path| {
        path.file_name() == Some(OsStr::new("Cargo.toml"))
    });
    for path in manifest_files {
        let text = fs::read_to_string(&path).expect("read Cargo.toml");
        assert!(
            !contains_word(&text, "tonic") && !contains_word(&text, "grpc"),
            "{} must not introduce gRPC/tonic dependencies",
            path.display()
        );
        assert!(
            !contains_word(&text, "serde_json"),
            "{} must not introduce serde_json runtime dependency",
            path.display()
        );
    }

    let production_sources = collect_files(&crate_sources, |path| {
        path.extension() == Some(OsStr::new("rs"))
            && path
                .components()
                .all(|component| component.as_os_str() != "tests")
    });
    for path in production_sources {
        let text = fs::read_to_string(&path).expect("read Rust source");
        assert_no_source_token(&path, &text, "tonic::");
        assert_no_source_token(&path, &text, "grpc::");
        assert_no_source_token(&path, &text, "serde_json");
        assert_no_source_token(&path, &text, "json!(");
        assert_no_source_token(&path, &text, "unsafe {");
        assert_no_source_token(&path, &text, "unsafe fn");
        assert_no_source_token(&path, &text, "unsafe impl");
    }

    let proto_files = collect_files(&proto_schemas, |path| {
        path.extension() == Some(OsStr::new("proto"))
    });
    for path in proto_files {
        let text = fs::read_to_string(&path).expect("read proto schema");
        for line in text.lines() {
            let trimmed = line.trim_start();
            assert!(
                !trimmed.starts_with("service ") && !trimmed.starts_with("rpc "),
                "{} must not define gRPC service/rpc declarations",
                path.display()
            );
        }
    }
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command.args(args).output().expect("run andromeda-cli")
}

fn run_cli_with_path<const N: usize>(args: [&str; N], path: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_andromeda-cli"));
    command
        .args(args)
        .arg(path)
        .output()
        .expect("run andromeda-cli")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is utf8")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn unique_temp_wal_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}.wal", std::process::id()))
}

fn collect_files(root: &Path, include: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_inner(root, &include, &mut files);
    files
}

fn collect_files_inner(root: &Path, include: &impl Fn(&Path) -> bool, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        if path.file_name() == Some(OsStr::new("target"))
            || path.file_name() == Some(OsStr::new(".git"))
        {
            continue;
        }

        if path.is_dir() {
            collect_files_inner(&path, include, files);
        } else if include(&path) {
            files.push(path);
        }
    }
}

fn assert_no_source_token(path: &Path, text: &str, token: &str) {
    assert!(
        !text.contains(token),
        "{} must not contain `{token}`",
        path.display()
    );
}

fn contains_word(text: &str, word: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token.eq_ignore_ascii_case(word))
}
