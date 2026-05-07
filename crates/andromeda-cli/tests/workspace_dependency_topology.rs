#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const CONCRETE_RUNTIME_TLS_QUIC_DEPS: [&str; 3] = ["quinn", "rcgen", "rustls"];
const RPC_PROTOCOL_FORBIDDEN_RUNTIME_DEPS: [&str; 4] = ["quinn", "rcgen", "rustls", "tokio"];
const RPC_PROTOCOL_FORBIDDEN_SOURCE_TOKENS: [&str; 8] = [
    "quinn::",
    "rcgen::",
    "rustls::",
    "tokio::",
    "runtime_quinn",
    "quinn_backend",
    "quinn_tls",
    "#[tokio::",
];

#[test]
fn workspace_crate_dependency_topology_blocks_forbidden_runtime_edges() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let violations = forbidden_rules()
        .iter()
        .flat_map(|rule| rule.violations(&manifests))
        .chain(
            forbidden_rules()
                .iter()
                .flat_map(|rule| rule.transitive_violations(&manifests)),
        )
        .chain(
            allowed_dependency_rules()
                .iter()
                .flat_map(|rule| rule.violations(&manifests)),
        )
        .chain(
            allowed_dev_dependency_rules()
                .iter()
                .flat_map(|rule| rule.violations(&manifests)),
        )
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "workspace dependency topology violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn only_andromeda_quic_declares_concrete_runtime_tls_quic_crates() {
    let manifests = load_crate_manifests(&workspace_root().join("crates"));
    let mut violations = Vec::new();

    for manifest in manifests.values() {
        if manifest.package_name == "andromeda-quic" {
            continue;
        }

        for dependency in &manifest.runtime_dependencies {
            if CONCRETE_RUNTIME_TLS_QUIC_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{} declares production dependency `{dependency}` in {}; concrete Quinn/TLS crates must stay owned by andromeda-quic until runtime extraction",
                    manifest.package_name,
                    manifest.path.display()
                ));
            }
        }

        for dependency in &manifest.dev_dependencies {
            if CONCRETE_RUNTIME_TLS_QUIC_DEPS.contains(&dependency.as_str()) {
                violations.push(format!(
                    "{} declares dev dependency `{dependency}` in {}; concrete Quinn/TLS crates must stay owned by andromeda-quic until runtime extraction",
                    manifest.package_name,
                    manifest.path.display()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "concrete runtime TLS/QUIC dependency ownership violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn rpc_protocol_crate_stays_runtime_free_in_manifest_and_source() {
    let workspace = workspace_root();
    let manifests = load_crate_manifests(&workspace.join("crates"));
    let manifest = manifests
        .get("andromeda-rpc-protocol")
        .expect("workspace must include andromeda-rpc-protocol");
    let mut violations = Vec::new();

    for dependency in manifest
        .runtime_dependencies
        .iter()
        .chain(manifest.dev_dependencies.iter())
    {
        if RPC_PROTOCOL_FORBIDDEN_RUNTIME_DEPS.contains(&dependency.as_str()) {
            violations.push(format!(
                "andromeda-rpc-protocol manifest must not depend on runtime crate `{dependency}`"
            ));
        }
    }

    let rpc_protocol_src = workspace.join("crates/andromeda-rpc-protocol/src");
    for file in rust_source_files(&rpc_protocol_src) {
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        let code_without_comments = strip_rust_comments(&source);
        let relative = relative_slash_path(&workspace, &file);

        for (line_index, line) in code_without_comments.lines().enumerate() {
            let compact_line = line
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>();

            for token in RPC_PROTOCOL_FORBIDDEN_SOURCE_TOKENS {
                if compact_line.contains(token) {
                    violations.push(format!(
                        "{relative}:{} exposes runtime token `{token}`",
                        line_index + 1
                    ));
                }
            }

            if compact_line.starts_with("pubmodruntime") || compact_line.starts_with("modruntime") {
                violations.push(format!(
                    "{relative}:{} declares a runtime module from the abstract protocol crate",
                    line_index + 1
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "andromeda-rpc-protocol must remain runtime-free:\n{}",
        violations.join("\n")
    );
}

#[test]
fn core_facade_does_not_grow_new_local_modules_during_foundation_migration() {
    let core_src = workspace_root().join("crates/andromeda-core/src");
    let actual_entries = fs::read_dir(&core_src)
        .expect("read andromeda-core src directory")
        .map(|entry| {
            entry
                .expect("read andromeda-core src entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<BTreeSet<_>>();
    let expected_entries = ["lib.rs", "principal"]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        actual_entries, expected_entries,
        "andromeda-core must stay a temporary facade in batch 1; add new foundation code to the extracted crates or document a later-batch migration exception"
    );
}

fn forbidden_rules() -> Vec<ForbiddenRule> {
    vec![
        ForbiddenRule::new(
            "Foundation crates and the temporary core facade must not depend on higher-level engine crates",
            &[
                "andromeda-error",
                "andromeda-digest",
                "andromeda-types",
                "andromeda-time",
                "andromeda-hardware",
                "andromeda-core",
            ],
            &[
                "andromeda-catalog",
                "andromeda-srpl",
                "andromeda-proto",
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-exec",
                "andromeda-observe",
                "andromeda-quic",
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
                "quinn",
            ],
        ),
        ForbiddenRule::new(
            "C5 and durable kernel crates must not depend on GPU, analytics, or benchmark crates",
            &[
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-wal",
                "andromeda-recovery",
                "andromeda-cold-store",
                "andromeda-hot-store",
            ],
            &[
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
            ],
        ),
        ForbiddenRule::new(
            "WAL, storage, and recovery crates must not depend on concrete network runtime or RPC runtime crates",
            &[
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-wal",
                "andromeda-recovery",
            ],
            &[
                "andromeda-quic",
                "andromeda-rpc-runtime",
                "andromeda-runtime-quinn",
                "quinn",
                "rustls",
                "tokio-rustls",
                "h2",
                "hyper",
                "tower",
            ],
        ),
        ForbiddenRule::new(
            "Durable kernel crates must not depend on SRPL, catalog store, execution, or protocol runtime crates",
            &[
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-wal",
                "andromeda-recovery",
                "andromeda-cold-store",
                "andromeda-hot-store",
                "andromeda-buffer-pool",
                "andromeda-storage-page",
                "andromeda-storage-layout",
                "andromeda-tx-mvcc",
            ],
            &[
                "andromeda-catalog",
                "andromeda-catalog-store",
                "andromeda-catalog-runtime",
                "andromeda-srpl",
                "andromeda-srpl-parser",
                "andromeda-srpl-ast",
                "andromeda-srpl-ir",
                "andromeda-srpl-diagnostics",
                "andromeda-proto",
                "andromeda-quic",
                "andromeda-rpc",
                "andromeda-rpc-contract",
                "andromeda-rpc-runtime",
                "andromeda-runtime-quinn",
                "andromeda-exec",
                "andromeda-execution",
                "quinn",
                "rustls",
                "tokio-rustls",
                "h2",
                "hyper",
                "tower",
                "prost",
                "prost-types",
                "prost-build",
                "protoc-bin-vendored",
            ],
        ),
        ForbiddenRule::new(
            "Durable kernel crates must not use implicit native layout, SQL, or runtime JSON serialization dependencies",
            &[
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-wal",
                "andromeda-recovery",
                "andromeda-cold-store",
                "andromeda-hot-store",
                "andromeda-buffer-pool",
                "andromeda-storage-page",
                "andromeda-storage-layout",
                "andromeda-tx-mvcc",
            ],
            &[
                "serde",
                "serde-json",
                "bincode",
                "rkyv",
                "bytemuck",
                "zerocopy",
                "sqlx",
                "rusqlite",
                "diesel",
            ],
        ),
        ForbiddenRule::new(
            "Catalog and Procedure contract crates must not depend on execution crates",
            &[
                "andromeda-catalog",
                "andromeda-contract",
                "andromeda-contracts",
                "andromeda-procedure-contract",
            ],
            &["andromeda-exec", "andromeda-execution"],
        ),
        ForbiddenRule::new(
            "Procedure contract crates must not depend on catalog, protocol, runtime, storage, analytics, or wire-generation crates",
            &["andromeda-contract"],
            &[
                "andromeda-core",
                "andromeda-catalog",
                "andromeda-contract",
                "andromeda-proto",
                "andromeda-srpl",
                "andromeda-observe",
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-exec",
                "andromeda-quic",
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
                "prost",
                "prost-build",
                "protoc-bin-vendored",
                "quinn",
                "tokio",
            ],
        ),
        ForbiddenRule::new(
            "StructuredObject contract model must not depend on protocol, runtime, storage, analytics, or wire-generation crates",
            &["andromeda-structured-object"],
            &[
                "andromeda-core",
                "andromeda-contract",
                "andromeda-catalog",
                "andromeda-proto",
                "andromeda-srpl",
                "andromeda-observe",
                "andromeda-storage",
                "andromeda-tx",
                "andromeda-exec",
                "andromeda-quic",
                "andromeda-bench",
                "andromeda-analytics",
                "andromeda-gpu",
                "andromeda-gpu-kernels",
                "prost",
                "prost-types",
                "prost-build",
                "protoc-bin-vendored",
                "quinn",
                "tokio",
                "serde",
                "serde-json",
                "sqlx",
                "rusqlite",
                "diesel",
            ],
        ),
        ForbiddenRule::new(
            "SRPL parser and language-model crates must not depend on catalog store implementation crates",
            &[
                "andromeda-srpl-parser",
                "andromeda-srpl-ast",
                "andromeda-srpl-lexer",
                "andromeda-srpl-diagnostics",
                "andromeda-srpl-cardinality",
                "andromeda-srpl-ir",
            ],
            &[
                "andromeda-catalog",
                "andromeda-catalog-store",
                "andromeda-catalog-runtime",
            ],
        ),
        ForbiddenRule::new(
            "Abstract RPC and protocol contract crates must not depend on concrete network runtime crates",
            &[
                "andromeda-proto",
                "andromeda-protocol",
                "andromeda-protocol-contract",
                "andromeda-rpc",
                "andromeda-rpc-abstract",
                "andromeda-rpc-protocol",
                "andromeda-rpc-contract",
                "andromeda-rpc-surface",
            ],
            &[
                "andromeda-quic",
                "andromeda-runtime-quinn",
                "quinn",
                "rcgen",
                "rustls",
                "tokio",
            ],
        ),
        ForbiddenRule::new(
            "Application RPC surface crates must not depend on administration, cluster, HA/DR, or backup runtime crates",
            &[
                "andromeda-application",
                "andromeda-application-rpc",
                "andromeda-application-surface",
                "andromeda-rpc-application",
                "andromeda-rpc-application-surface",
            ],
            &[
                "andromeda-admin",
                "andromeda-admin-runtime",
                "andromeda-administration",
                "andromeda-administration-runtime",
                "andromeda-backup",
                "andromeda-backup-runtime",
                "andromeda-cluster",
                "andromeda-cluster-runtime",
                "andromeda-hadr",
                "andromeda-hadr-runtime",
            ],
        ),
    ]
}

fn allowed_dependency_rules() -> Vec<AllowedDependencyRule> {
    vec![
        AllowedDependencyRule::new(
            "andromeda-contract may only depend on contract-safe R0 crates",
            "andromeda-contract",
            &["andromeda-digest", "andromeda-error", "andromeda-types"],
        ),
        AllowedDependencyRule::new(
            "andromeda-structured-object may only depend on contract-safe R0 crates",
            "andromeda-structured-object",
            &["andromeda-digest", "andromeda-error", "andromeda-types"],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-diagnostics may only depend on foundation error handling",
            "andromeda-srpl-diagnostics",
            &["andromeda-error"],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-cardinality may only depend on contract-safe cardinality types",
            "andromeda-srpl-cardinality",
            &["andromeda-contract"],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-ast may only depend on parser-safe language model crates",
            "andromeda-srpl-ast",
            &[
                "andromeda-contract",
                "andromeda-srpl-cardinality",
                "andromeda-srpl-diagnostics",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-parser may only depend on parser-safe language model crates",
            "andromeda-srpl-parser",
            &[
                "andromeda-contract",
                "andromeda-srpl-ast",
                "andromeda-srpl-cardinality",
                "andromeda-srpl-diagnostics",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-srpl-ir may only depend on contract-safe semantic model crates",
            "andromeda-srpl-ir",
            &[
                "andromeda-contract",
                "andromeda-error",
                "andromeda-srpl-cardinality",
                "andromeda-types",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-rpc-protocol may only depend on runtime-free protocol foundation crates",
            "andromeda-rpc-protocol",
            &["andromeda-core"],
        ),
        AllowedDependencyRule::new(
            "andromeda-storage may only depend on current Lot 4.3 durable-kernel support crates",
            "andromeda-storage",
            &[
                "andromeda-core",
                "andromeda-observe",
                "andromeda-wal",
                "dashmap",
                "sha2",
            ],
        ),
        AllowedDependencyRule::new(
            "andromeda-wal may only depend on pure WAL foundation crates",
            "andromeda-wal",
            &["andromeda-core"],
        ),
        AllowedDependencyRule::new(
            "andromeda-tx may only depend on current Lot 4.0 transaction-kernel support crates",
            "andromeda-tx",
            &[
                "andromeda-core",
                "andromeda-observe",
                "async-trait",
                "dashmap",
                "futures",
                "tokio",
            ],
        ),
    ]
}

fn allowed_dev_dependency_rules() -> Vec<AllowedDependencyRule> {
    vec![
        AllowedDependencyRule::new_for_scope(
            "andromeda-storage may only dev-depend on storage test harness crates",
            "andromeda-storage",
            DependencyScope::Dev,
            &["proptest", "tempfile"],
        ),
        AllowedDependencyRule::new_for_scope(
            "andromeda-wal may only dev-depend on pure WAL test harness crates",
            "andromeda-wal",
            DependencyScope::Dev,
            &["proptest"],
        ),
    ]
}

struct ForbiddenRule {
    message: &'static str,
    sources: BTreeSet<&'static str>,
    forbidden_dependencies: BTreeSet<&'static str>,
}

impl ForbiddenRule {
    fn new(
        message: &'static str,
        sources: &[&'static str],
        forbidden_dependencies: &[&'static str],
    ) -> Self {
        Self {
            message,
            sources: sources.iter().copied().collect(),
            forbidden_dependencies: forbidden_dependencies.iter().copied().collect(),
        }
    }

    fn violations(&self, manifests: &BTreeMap<String, CrateManifest>) -> Vec<String> {
        manifests
            .values()
            .filter(|manifest| self.sources.contains(manifest.package_name.as_str()))
            .flat_map(|manifest| {
                manifest
                    .runtime_dependencies
                    .iter()
                    .filter(|dependency| self.forbidden_dependencies.contains(dependency.as_str()))
                    .map(|dependency| {
                        format!(
                            "{}: `{}` must not depend on `{}` in {}",
                            self.message,
                            manifest.package_name,
                            dependency,
                            manifest.path.display()
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn transitive_violations(&self, manifests: &BTreeMap<String, CrateManifest>) -> Vec<String> {
        manifests
            .values()
            .filter(|manifest| self.sources.contains(manifest.package_name.as_str()))
            .flat_map(|manifest| self.transitive_violations_for_manifest(manifest, manifests))
            .collect()
    }

    fn transitive_violations_for_manifest(
        &self,
        manifest: &CrateManifest,
        manifests: &BTreeMap<String, CrateManifest>,
    ) -> Vec<String> {
        let mut violations = Vec::new();
        let mut visited = BTreeSet::new();
        let mut pending = manifest
            .runtime_dependencies
            .iter()
            .map(|dependency| vec![manifest.package_name.clone(), dependency.clone()])
            .collect::<Vec<_>>();

        while let Some(path) = pending.pop() {
            let dependency = path.last().expect("path contains dependency").clone();

            if path.len() > 2 && self.forbidden_dependencies.contains(dependency.as_str()) {
                violations.push(format!(
                    "{}: `{}` must not transitively depend on `{}` through `{}` in {}",
                    self.message,
                    manifest.package_name,
                    dependency,
                    path.join(" -> "),
                    manifest.path.display()
                ));
                continue;
            }

            if !visited.insert(dependency.clone()) {
                continue;
            }

            let Some(dependency_manifest) = manifests.get(&dependency) else {
                continue;
            };

            for child_dependency in &dependency_manifest.runtime_dependencies {
                let mut child_path = path.clone();
                child_path.push(child_dependency.clone());
                pending.push(child_path);
            }
        }

        violations
    }
}

struct AllowedDependencyRule {
    message: &'static str,
    source: &'static str,
    scope: DependencyScope,
    allowed_dependencies: BTreeSet<&'static str>,
}

impl AllowedDependencyRule {
    fn new(
        message: &'static str,
        source: &'static str,
        allowed_dependencies: &[&'static str],
    ) -> Self {
        Self::new_for_scope(
            message,
            source,
            DependencyScope::Runtime,
            allowed_dependencies,
        )
    }

    fn new_for_scope(
        message: &'static str,
        source: &'static str,
        scope: DependencyScope,
        allowed_dependencies: &[&'static str],
    ) -> Self {
        Self {
            message,
            source,
            scope,
            allowed_dependencies: allowed_dependencies.iter().copied().collect(),
        }
    }

    fn violations(&self, manifests: &BTreeMap<String, CrateManifest>) -> Vec<String> {
        let Some(manifest) = manifests.get(self.source) else {
            return Vec::new();
        };

        self.dependencies_for(manifest)
            .iter()
            .filter(|dependency| !self.allowed_dependencies.contains(dependency.as_str()))
            .map(|dependency| {
                format!(
                    "{}: `{}` must not depend on `{}` in {}",
                    self.message,
                    manifest.package_name,
                    dependency,
                    manifest.path.display()
                )
            })
            .collect()
    }

    fn dependencies_for<'a>(&self, manifest: &'a CrateManifest) -> &'a BTreeSet<String> {
        match self.scope {
            DependencyScope::Runtime => &manifest.runtime_dependencies,
            DependencyScope::Dev => &manifest.dev_dependencies,
        }
    }
}

#[derive(Clone, Copy)]
enum DependencyScope {
    Runtime,
    Dev,
}

struct CrateManifest {
    package_name: String,
    path: PathBuf,
    runtime_dependencies: BTreeSet<String>,
    dev_dependencies: BTreeSet<String>,
}

fn load_crate_manifests(root: &Path) -> BTreeMap<String, CrateManifest> {
    let mut manifests = BTreeMap::new();

    for path in collect_manifest_paths(root) {
        let text = fs::read_to_string(&path).expect("read crate Cargo.toml");
        let package_name = parse_package_name(&text)
            .unwrap_or_else(|| panic!("{} must define package.name", path.display()));
        let dependencies = parse_dependency_names(&text);

        manifests.insert(
            package_name.clone(),
            CrateManifest {
                package_name,
                path,
                runtime_dependencies: dependencies.runtime,
                dev_dependencies: dependencies.dev,
            },
        );
    }

    manifests
}

fn collect_manifest_paths(root: &Path) -> Vec<PathBuf> {
    let mut manifests = Vec::new();
    let entries = fs::read_dir(root).expect("read crates directory");

    for entry in entries {
        let entry = entry.expect("read crates directory entry");
        let path = entry.path();
        if path.is_dir() {
            let manifest = path.join("Cargo.toml");
            if manifest.exists() {
                manifests.push(manifest);
            }
        }
    }

    manifests
}

fn rust_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_source_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_source_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("failed to read directory {}: {err}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|err| {
            panic!("failed to read directory entry in {}: {err}", dir.display())
        });
        let path = entry.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn relative_slash_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn strip_rust_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0_usize;
    let mut in_line_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                output.push('\n');
            } else {
                output.push(' ');
            }
            continue;
        }

        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                    output.push_str("  ");
                }
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                    output.push_str("  ");
                }
                ('\n', _) => output.push('\n'),
                _ => output.push(' '),
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('/', Some('/')) => {
                chars.next();
                in_line_comment = true;
                output.push_str("  ");
            }
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
                output.push_str("  ");
            }
            _ => output.push(ch),
        }
    }

    output
}

fn parse_package_name(text: &str) -> Option<String> {
    let mut in_package = false;

    for line in text.lines() {
        let Some(line) = cargo_line_without_comment(line) else {
            continue;
        };

        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }

        if in_package && line.starts_with("name") {
            return parse_quoted_value(line);
        }
    }

    None
}

#[derive(Default)]
struct ManifestDependencies {
    runtime: BTreeSet<String>,
    dev: BTreeSet<String>,
}

fn parse_dependency_names(text: &str) -> ManifestDependencies {
    let mut dependencies = ManifestDependencies::default();
    let mut dependency_section = ParsedDependencySection::None;

    for line in text.lines() {
        let Some(line) = cargo_line_without_comment(line) else {
            continue;
        };

        if line.starts_with('[') {
            dependency_section = dependency_section_for(line);
            continue;
        }

        if matches!(dependency_section, ParsedDependencySection::None) {
            continue;
        }

        if let Some((name, rest)) = line.split_once('=') {
            let dependency = normalize_dependency_name(name.trim());
            dependency_section.insert(&mut dependencies, dependency);

            if let Some(package) = parse_package_rename(rest) {
                dependency_section.insert(&mut dependencies, normalize_dependency_name(&package));
            }
        }
    }

    dependencies
}

#[derive(Clone, Copy)]
enum ParsedDependencySection {
    None,
    Runtime,
    Dev,
}

impl ParsedDependencySection {
    fn insert(self, dependencies: &mut ManifestDependencies, dependency: String) {
        match self {
            ParsedDependencySection::None => {}
            ParsedDependencySection::Runtime => {
                dependencies.runtime.insert(dependency);
            }
            ParsedDependencySection::Dev => {
                dependencies.dev.insert(dependency);
            }
        }
    }
}

fn dependency_section_for(line: &str) -> ParsedDependencySection {
    if line == "[dependencies]"
        || line == "[build-dependencies]"
        || line.starts_with("[target.") && line.ends_with(".dependencies]")
        || line.starts_with("[target.") && line.ends_with(".build-dependencies]")
    {
        return ParsedDependencySection::Runtime;
    }

    if line == "[dev-dependencies]"
        || line.starts_with("[target.") && line.ends_with(".dev-dependencies]")
    {
        return ParsedDependencySection::Dev;
    }

    ParsedDependencySection::None
}

fn cargo_line_without_comment(line: &str) -> Option<&str> {
    let line = line.split('#').next().unwrap_or_default().trim();
    (!line.is_empty()).then_some(line)
}

fn parse_package_rename(text: &str) -> Option<String> {
    text.split(',')
        .map(|part| {
            part.trim()
                .trim_matches(|character| character == '{' || character == '}')
        })
        .map(str::trim)
        .find(|part| part.starts_with("package"))
        .and_then(parse_quoted_value)
}

fn parse_quoted_value(text: &str) -> Option<String> {
    let (_, value) = text.split_once('=')?;
    let value = value.trim();
    let value = value.strip_prefix('"')?;
    let value = value.split_once('"')?.0;
    Some(value.to_owned())
}

fn normalize_dependency_name(name: &str) -> String {
    let name = name.trim_matches(|character: char| {
        character == '"' || character == '\'' || character.is_whitespace()
    });
    let name = name.split_once('.').map_or(name, |(key, _)| key);
    name.replace('_', "-").to_ascii_lowercase()
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn manifest_parser_tracks_package_renames_in_dependency_sections() {
    let dependencies = parse_dependency_names(
        r#"
        [package]
        name = "sample"

        [dependencies]
        transport = { package = "quinn", workspace = true }
        ignored = { package = "andromeda-exec", workspace = true }

        [dev-dependencies]
        temp = { package = "tempfile", workspace = true }
        proptest.workspace = true

        [build-dependencies]
        proto = { package = "prost-build", workspace = true }

        [workspace.dependencies]
        andromeda-bench = { path = "crates/andromeda-bench" }
        "#,
    );

    assert!(dependencies.runtime.contains("quinn"));
    assert!(dependencies.runtime.contains("andromeda-exec"));
    assert!(dependencies.runtime.contains("prost-build"));
    assert!(dependencies.dev.contains("tempfile"));
    assert!(dependencies.dev.contains("proptest"));
    assert!(!dependencies.runtime.contains("andromeda-bench"));
    assert!(!dependencies.dev.contains("andromeda-bench"));
}

#[test]
fn forbidden_rules_report_transitive_dependency_paths() {
    let manifests = BTreeMap::from([
        (
            "andromeda-storage".to_owned(),
            test_manifest(
                "andromeda-storage",
                &["andromeda-observe"],
                "crates/andromeda-storage/Cargo.toml",
            ),
        ),
        (
            "andromeda-observe".to_owned(),
            test_manifest(
                "andromeda-observe",
                &["andromeda-quic"],
                "crates/andromeda-observe/Cargo.toml",
            ),
        ),
        (
            "andromeda-quic".to_owned(),
            test_manifest(
                "andromeda-quic",
                &["quinn"],
                "crates/andromeda-quic/Cargo.toml",
            ),
        ),
    ]);
    let rule = ForbiddenRule::new(
        "WAL, storage, and recovery crates must not depend on Quinn or RPC runtime crates",
        &["andromeda-storage"],
        &["andromeda-quic", "quinn"],
    );

    let violations = rule.transitive_violations(&manifests);

    assert_eq!(violations.len(), 1);
    assert!(
        violations[0].contains("andromeda-storage -> andromeda-observe -> andromeda-quic"),
        "{violations:?}"
    );
}

fn test_manifest(name: &str, runtime_dependencies: &[&str], path: &str) -> CrateManifest {
    CrateManifest {
        package_name: name.to_owned(),
        path: PathBuf::from(path),
        runtime_dependencies: runtime_dependencies
            .iter()
            .map(|dependency| dependency.to_string())
            .collect(),
        dev_dependencies: BTreeSet::new(),
    }
}
