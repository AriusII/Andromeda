# Validation Manifest Schema

## Purpose

Define the JSON shape emitted by `tools/testing/validation_manifest.py`.

The validation manifest is a read-only local aggregation report for Andromeda validation readiness. It summarizes whether key documentation, specification, crate, fuzz, runbook, and test index files exist, and it reports known blockers that must not be interpreted as release approval.

## Scope

This schema covers `andromeda.validation_manifest.v1`, emitted with:

```powershell
python tools/testing/validation_manifest.py --json
```

The manifest includes:

- index presence checks for `docs`, `specs`, `crates`, `fuzz`, `runbooks`, and `tests`;
- local inventory counts for documentation files, specification files, workspace crates, fuzz targets, operations runbooks, root test indices, crate test files, and Loom evidence paths;
- known blocker records gathered from required-path gaps, local consistency checks, and standing release-readiness gaps documented in the repository.

## Non-goals

The validation manifest does not:

- run Rust tests, fuzz jobs, Miri, Loom, cargo audit, cargo deny, or CI workflows;
- create, update, delete, stage, commit, or clean repository files;
- treat documentation, corpus, benchmark, GPU, RAM, generated, or temporary output as durable truth;
- approve release readiness;
- replace crate-owned validation suites or release evidence records;
- prove crash/recovery behavior for C4 or C5 claims.

## Prerequisites

- Python 3 with only the standard library.
- Run the script from any directory, or pass `--root` to point at the repository root.
- Keep the repository indices current before treating the manifest as useful planning evidence.

## Procedure

Emit text output:

```powershell
python tools/testing/validation_manifest.py
```

Emit JSON output:

```powershell
python tools/testing/validation_manifest.py --json
```

Use strict mode only when an automation should fail on missing required indices or known blockers:

```powershell
python tools/testing/validation_manifest.py --strict
python tools/testing/validation_manifest.py --json --strict
```

## JSON Fields

The top-level object has these fields.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `schema` | string | Yes | Schema identifier. Current value is `andromeda.validation_manifest.v1`. |
| `root` | string | Yes | Absolute repository root used for all path checks. |
| `status` | string | Yes | `PASS`, `BLOCKED`, or `FAIL`. `FAIL` means at least one required index is missing. `BLOCKED` means required indices are present but known blockers exist. |
| `summary` | array | Yes | Per-category index presence totals. |
| `indices` | array | Yes | Individual index presence results. |
| `inventory` | object | Yes | Category-specific local inventory counts and evidence lists. |
| `known_blockers` | array | Yes | Structured blocker records. Empty only when no missing required indices, local consistency blockers, or standing release blockers are known. |

## `summary`

Each item in `summary` has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `category` | string | One of `docs`, `specs`, `crates`, `fuzz`, `runbooks`, or `tests`. |
| `total` | integer | Number of index checks in the category. |
| `present` | integer | Number of index checks whose path exists. |
| `missing` | array of strings | Missing index paths in the category. |
| `required_missing` | array of strings | Missing paths that are required for the manifest. |

## `indices`

Each item in `indices` has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `category` | string | Validation category. |
| `label` | string | Human-readable index name. |
| `path` | string | Repository-relative path checked by the script. |
| `required` | boolean | Whether a missing path changes the top-level status to `FAIL`. |
| `present` | boolean | Whether the path exists under `root`. |

## `inventory`

The `inventory` object contains one object per validation category.

### `inventory.docs`

| Field | Type | Description |
| --- | --- | --- |
| `documentations_markdown_files` | integer | Markdown files under `documentations`, excluding ignored build paths. |
| `docs_markdown_files` | integer | Markdown files under `docs`, excluding ignored build paths. |
| `top_level_indices` | array of strings | Present documentation index paths from the `docs` category. |

### `inventory.specs`

| Field | Type | Description |
| --- | --- | --- |
| `specification_files` | integer | Specification Markdown files under `documentations/specs`, excluding `index.md`. |
| `index` | string | Specification index path. |
| `sample` | array of strings | Up to 12 specification paths for quick inspection. |

### `inventory.crates`

| Field | Type | Description |
| --- | --- | --- |
| `workspace_members` | integer | Number of `crates/*` members listed in the root workspace manifest. |
| `crate_directories` | integer | Number of directories directly under `crates`. |
| `crate_readmes` | integer | Number of crate README files found under `crates`. |
| `missing_member_manifests` | array of strings | Workspace members whose `Cargo.toml` is missing. |
| `non_workspace_crate_dirs` | array of strings | Crate directories that are not root workspace members. |

### `inventory.fuzz`

| Field | Type | Description |
| --- | --- | --- |
| `registered_targets` | integer | Number of `[[target]]` entries in `fuzz/targets.toml`. |
| `cargo_bins` | integer | Number of fuzz `[[bin]]` entries in `fuzz/Cargo.toml`. |
| `corpus_manifest_entries` | integer | Number of `[[entry]]` records in `fuzz/corpus/manifest.toml`. |
| `target_sources` | integer | Number of Rust fuzz target source files, excluding files declared as `[[support]]` in `fuzz/targets.toml`. |
| `targets` | array | Per-target source, corpus, generator, Cargo bin, corpus manifest, and invalid-input policy status. |

Each item in `inventory.fuzz.targets` has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | Fuzz target name from `fuzz/targets.toml`. |
| `source` | string | Repository-relative source path. |
| `source_present` | boolean | Whether the source path exists. |
| `corpus_dir` | string | Corpus directory from the registry. |
| `corpus_present` | boolean | Whether the corpus directory exists. |
| `generator` | string | Seed generator path from the registry. |
| `generator_present` | boolean | Whether the seed generator exists. |
| `cargo_bin_present` | boolean | Whether `fuzz/Cargo.toml` has a matching `[[bin]]`. |
| `corpus_manifest_present` | boolean | Whether `fuzz/corpus/manifest.toml` has a matching entry. |
| `invalid_input_policy` | string | Policy value from the target registry. Values that mention `stub` are reported as known blockers before promotion. |

### `inventory.runbooks`

| Field | Type | Description |
| --- | --- | --- |
| `runbook_files` | integer | Markdown runbooks under `documentations/operations/runbooks`, excluding `index.md`. |
| `index` | string | Runbook index path. |
| `runbooks` | array of strings | Repository-relative runbook paths. |

### `inventory.tests`

| Field | Type | Description |
| --- | --- | --- |
| `root_test_indices` | array of strings | `README.md` files under root `tests` index directories. |
| `root_test_index_count` | integer | Number of root test index README files. |
| `crate_test_rust_files` | integer | Rust files under crate-owned `tests` paths. |
| `loom_model_paths` | array of strings | Paths where the script detected Loom dependency or model evidence. |

## `known_blockers`

Each item in `known_blockers` has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `category` | string | Validation category associated with the blocker. |
| `severity` | string | `critical`, `high`, or `medium`. |
| `source` | string | Repository-relative source path or registry path that explains the blocker. |
| `message` | string | Human-readable blocker statement. |

Blockers can come from three sources:

- required index paths that are missing;
- local consistency checks, such as a fuzz target missing its source, corpus, generator, Cargo bin, corpus manifest entry, or seed file;
- standing release blockers documented in the repository, such as advisory Miri output, absent Loom model evidence, Map publication owner-suite gaps, backup/restore drill gaps, B-Tree durability gaps, and missing release evidence artifacts.

## Validation

Validate the script syntax:

```powershell
python -m py_compile tools/testing/validation_manifest.py
```

Run the read-only text report:

```powershell
python tools/testing/validation_manifest.py
```

Run the read-only JSON report:

```powershell
python tools/testing/validation_manifest.py --json
```

Use `--strict` only for automation that intentionally fails while blockers remain.

## Troubleshooting

If `status` is `FAIL`, inspect `summary[*].required_missing` and restore or update the missing index path.

If `status` is `BLOCKED`, inspect `known_blockers`. A blocker can be a real missing file, a registry mismatch, or a documented release gap that still needs retained evidence.

If a fuzz target reports a missing corpus seed, reconcile `fuzz/targets.toml`, `fuzz/Cargo.toml`, `fuzz/corpus/manifest.toml`, and `fuzz/generators/generate_seed_corpus.py` before accepting release evidence.

If `loom_model_paths` is empty, do not use the manifest to approve concurrency-sensitive C5 claims. Add owner-crate Loom evidence or explicitly scope the claim away from that concurrency surface.

## References

- `AGENTS.md`
- `tools/testing/validation_manifest.py`
- `tools/testing/README.md`
- `tests/README.md`
- `documentations/testing/index.md`
- `documentations/testing/step-11-validation-matrix.md`
- `documentations/testing/ci-release-gate-evidence.md`
- `documentations/specs/index.md`
- `documentations/operations/runbooks/index.md`
- `fuzz/targets.toml`
- `fuzz/corpus/manifest.toml`
