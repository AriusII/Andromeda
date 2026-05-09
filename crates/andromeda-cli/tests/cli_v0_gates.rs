#![forbid(unsafe_code)]

use andromeda_test_support::{
    process::{assert_success, run_binary, run_binary_with_path, stdout_utf8 as stdout},
    workspace::{unique_temp_path, workspace_root_from_manifest_dir},
};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

const FORBIDDEN_SQL_CLIENT_DEPS: &[&str] = &[
    "diesel",
    "mysql",
    "mysql_async",
    "odbc-api",
    "postgres",
    "rusqlite",
    "sea-orm",
    "sqlx",
    "tiberius",
    "tokio-postgres",
];

const FORBIDDEN_JSON_RUNTIME_DEPS: &[&str] = &["jsonrpsee", "serde_json"];

const FORBIDDEN_APPLICATION_SQL_SURFACE_TOKENS: &[&str] = &[
    "--sql",
    "execute_sql",
    "raw_sql",
    "query_text",
    "statement_text",
    "sql_text",
    "SqlCommand",
    "SqlQuery",
];

const FORBIDDEN_RUNTIME_JSON_SURFACE_TOKENS: &[&str] = &[
    "application/json",
    "jsonrpc",
    "JsonRpc",
    "json_wire",
    "JsonWire",
];

const FORBIDDEN_PROTO_RUNTIME_JSON_TOKENS: &[&str] = &[
    "google.protobuf.Struct",
    "google.protobuf.Value",
    "google.protobuf.ListValue",
    "google.protobuf.NullValue",
    "json_name",
];

#[test]
fn protocol_smoke_detail_output_is_deterministic() {
    let first = run_binary(cli_binary(), ["protocol-smoke", "--detail"]);
    let second = run_binary(cli_binary(), ["protocol-smoke", "--detail"]);
    assert_success(&first);
    assert_success(&second);

    let first_stdout = stdout(&first);
    let second_stdout = stdout(&second);

    assert_eq!(first_stdout, second_stdout);
    assert!(first_stdout.contains("payload/frame lockstep: ok (9 codes)"));
    assert!(first_stdout.contains("payload/frame codes:"));
    assert!(first_stdout.contains("representative ResultStream frames:"));
    assert!(first_stdout.contains("network sockets: not opened"));
}

#[test]
fn cli_human_surfaces_do_not_advertise_sql_grpc_or_default_json_runtime() {
    let help = run_binary(cli_binary(), ["--help"]);
    assert_success(&help);
    let help_stdout = stdout(&help);
    assert!(help_stdout.contains("durable audit trace inspection"));
    assert!(!help_stdout.contains("audit query"));
    assert!(!help_stdout.contains("durable audit trace queries"));
    assert_no_cli_surface_drift(&help_stdout);

    let audit_help = run_binary(cli_binary(), ["audit", "--help"]);
    assert_success(&audit_help);
    let audit_help_stdout = stdout(&audit_help);
    assert!(audit_help_stdout.contains("inspect  Inspect durable audit journal replay"));
    assert_no_cli_surface_drift(&audit_help_stdout);

    let protocol = run_binary(cli_binary(), ["protocol-smoke", "--detail"]);
    assert_success(&protocol);
    let protocol_stdout = stdout(&protocol);
    assert!(protocol_stdout.contains("typed ResultStream sequence: ok"));
    assert_no_cli_surface_drift(&protocol_stdout);
}

#[test]
fn inventory_recoverable_writes_file_wal_and_recovery_inspect_reports_replay() {
    let wal_path = unique_temp_path("andromeda-cli-v0", ".wal");
    fs::remove_file(&wal_path).ok();

    let vertical =
        run_binary_with_path(cli_binary(), ["inventory-recoverable", "--wal"], &wal_path);
    assert_success(&vertical);

    let vertical_stdout = stdout(&vertical);
    assert!(vertical_stdout.contains("Andromeda recoverable inventory procedure demo"));
    assert!(vertical_stdout.contains("status: Committed"));
    assert!(vertical_stdout.contains("rows affected: Some(2)"));
    assert!(vertical_stdout.contains("recovery replay LSNs: [2]"));
    assert!(vertical_stdout.contains("recovery boundary: Clean"));
    assert!(vertical_stdout.contains("forensic required: false"));
    assert!(vertical_stdout.contains("result frames: 3"));

    let inspect = run_binary_with_path(cli_binary(), ["recovery-inspect"], &wal_path);
    assert_success(&inspect);

    let inspect_stdout = stdout(&inspect);
    assert!(inspect_stdout.contains("Andromeda WAL recovery inspect"));
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
fn vertical_v0_alias_still_runs_recoverable_inventory_demo() {
    let wal_path = unique_temp_path("andromeda-cli-v0-alias", ".wal");
    fs::remove_file(&wal_path).ok();

    let vertical = run_binary_with_path(cli_binary(), ["vertical-v0", "--wal"], &wal_path);
    assert_success(&vertical);

    let vertical_stdout = stdout(&vertical);
    assert!(vertical_stdout.contains("Andromeda recoverable inventory procedure demo"));

    fs::remove_file(&wal_path).ok();
}

#[test]
fn workspace_policy_gates_do_not_drift_through_cli_scope() {
    let root = workspace_root();
    let crate_sources = root.join("crates");

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
        for dep in FORBIDDEN_SQL_CLIENT_DEPS {
            assert!(
                !contains_dependency_name(&text, dep),
                "{} must not introduce SQL client/runtime dependency `{dep}`; application traffic must stay Procedure-only",
                path.display()
            );
        }
        for dep in FORBIDDEN_JSON_RUNTIME_DEPS {
            assert!(
                !contains_dependency_name(&text, dep),
                "{} must not introduce JSON runtime dependency `{dep}`; runtime protocol payloads must stay typed and Protobuf-backed",
                path.display()
            );
        }
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
        for token in FORBIDDEN_APPLICATION_SQL_SURFACE_TOKENS {
            assert_no_source_token(&path, &text, token);
        }
        for token in FORBIDDEN_RUNTIME_JSON_SURFACE_TOKENS {
            assert_no_source_token(&path, &text, token);
        }
    }

    let proto_files = [
        root.join("schemas").join("proto"),
        root.join("crates").join("andromeda-proto").join("proto"),
    ]
    .into_iter()
    .flat_map(|root| collect_files(&root, |path| path.extension() == Some(OsStr::new("proto"))))
    .collect::<Vec<_>>();
    assert!(
        !proto_files.is_empty(),
        "workspace policy gate must scan normative Protobuf schemas"
    );
    for path in proto_files {
        let text = fs::read_to_string(&path).expect("read proto schema");
        for line in text.lines() {
            let trimmed = line.trim_start();
            assert!(
                !trimmed.starts_with("service ") && !trimmed.starts_with("rpc "),
                "{} must not define gRPC service/rpc declarations",
                path.display()
            );
            if !trimmed.starts_with("//") {
                for token in FORBIDDEN_PROTO_RUNTIME_JSON_TOKENS {
                    assert!(
                        !trimmed.contains(token),
                        "{} must not define runtime JSON-compatible Protobuf token `{token}`",
                        path.display()
                    );
                }
            }
        }
    }
}

fn cli_binary() -> &'static str {
    env!("CARGO_BIN_EXE_andromeda-cli")
}

fn workspace_root() -> PathBuf {
    workspace_root_from_manifest_dir(env!("CARGO_MANIFEST_DIR"))
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

fn assert_no_cli_surface_drift(text: &str) {
    let lower = text.to_ascii_lowercase();
    for token in [
        "--sql",
        "ad hoc sql",
        "audit query",
        "execute sql",
        "raw sql",
        "result set",
        "resultset",
        "trace query",
        "grpc",
        "tonic",
        "application/json",
        "jsonrpc",
        "default json",
        "json runtime",
    ] {
        assert!(
            !lower.contains(token),
            "CLI human surface must not advertise `{token}` as an Andromeda runtime surface:\n{text}"
        );
    }

    for token in ["query", "queries"] {
        assert!(
            !contains_word(&lower, token),
            "CLI human surface must not advertise `{token}` as native user wording:\n{text}"
        );
    }
}

fn contains_word(text: &str, word: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token.eq_ignore_ascii_case(word))
}

fn contains_dependency_name(text: &str, dependency: &str) -> bool {
    text.lines().any(|line| {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            return false;
        }
        let normalized = line.replace('_', "-").to_ascii_lowercase();
        let dependency = dependency.replace('_', "-").to_ascii_lowercase();
        normalized.starts_with(&format!("{dependency} "))
            || normalized.starts_with(&format!("{dependency}="))
            || normalized.contains(&format!("package = \"{dependency}\""))
            || normalized.contains(&format!("package='{dependency}'"))
    })
}
