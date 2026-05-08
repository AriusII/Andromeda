# Release Evidence Schema

## Purpose

Define the JSON shape emitted by `tools/testing/release_evidence.py`.

The local release evidence report captures repository metadata, toolchain metadata, and declared validation check results for an Andromeda release packet. The report is read-only by default and does not claim release readiness.

## Scope

This schema covers `andromeda.release_evidence.v1`, emitted with:

```powershell
python -B tools/testing/release_evidence.py --json
```

The report includes:

- local generator metadata;
- repository root, current working directory, Python runtime, and platform metadata;
- Git branch, commit, describe string, and dirty worktree counts;
- toolchain version command output for local Rust and release-gate tools;
- supplied check records from command-line arguments, JSON arguments, or JSON files;
- auto-detected skipped records for existing local scripts under `tools/testing`;
- status counts for pass, fail, skipped, gap, and partial records.

## Non-goals

The local release evidence report does not:

- run Rust tests, fuzz jobs, Miri, Loom, cargo audit, cargo deny, or CI workflows;
- create, update, delete, stage, commit, reset, clean, or otherwise modify repository files;
- write report files by default;
- approve release readiness;
- replace retained command transcripts, workflow artifacts, crash/recovery reports, or reviewer decisions;
- treat benchmark output, RAM state, GPU output, generated files, temporary files, or local caches as durable truth.

## Prerequisites

- Python 3 with only the standard library.
- Run from any directory, or pass `--root` to point at the repository root.
- Install optional tooling only when you want version metadata for that tool to appear as found.
- Run validation gates separately before marking a supplied check as `pass` or `fail`.

## Procedure

Emit the default text report:

```powershell
python -B tools/testing/release_evidence.py
```

Emit the JSON report:

```powershell
python -B tools/testing/release_evidence.py --json
```

Record a supplied check result:

```powershell
python -B tools/testing/release_evidence.py --json --check "id=fmt;status=pass;command=cargo fmt --all --check;artifact=artifacts/release/fmt.log;residual_risk=None recorded"
```

Record a release gap:

```powershell
python -B tools/testing/release_evidence.py --json --check "id=cr-11-wal;status=gap;command=cargo test -p andromeda-wal --tests --locked;residual_risk=No retained crash/recovery artifact for this release candidate"
```

Disable auto-detected script records when a caller wants only supplied checks:

```powershell
python -B tools/testing/release_evidence.py --json --no-auto-detect
```

Use strict mode only when a caller should fail on supplied `fail` or `gap` records:

```powershell
python -B tools/testing/release_evidence.py --json --strict
```

## JSON Fields

The top-level object has these fields.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `schema` | string | Yes | Schema identifier. Current value is `andromeda.release_evidence.v1`. |
| `generated_at_utc` | string | Yes | UTC timestamp for report generation in ISO 8601 format. |
| `generator` | object | Yes | Generator name, version, safety flags, and release-readiness disclaimer. |
| `environment` | object | Yes | Local runtime and platform metadata. |
| `git` | object | Yes | Git metadata and dirty worktree summary. |
| `toolchain` | array | Yes | Version-command results for local toolchain executables. |
| `checks` | array | Yes | Supplied and auto-detected check records. |
| `summary` | object | Yes | Check status totals and release-readiness disclaimer. |

## `generator`

The `generator` object has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | Script path. |
| `version` | string | Local generator schema implementation version. |
| `read_only` | boolean | Always `true` for this generator. |
| `writes_files` | boolean | Always `false`. The script writes only to stdout. |
| `runs_validation_gates` | boolean | Always `false`. Check commands are recorded, not executed. |
| `release_readiness_claim` | boolean | Always `false`. |
| `safety_note` | string | Human-readable scope and safety statement. |

## `environment`

The `environment` object has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `repository_root` | string | Absolute repository root used for metadata collection. |
| `cwd` | string | Current working directory when the script ran. |
| `platform` | string | Platform string from the Python runtime. |
| `python_executable` | string | Python executable used to run the script. |
| `python_version` | string | Python version. |

## `git`

The `git` object has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `available` | boolean | Whether `git` was found on `PATH`. |
| `root` | string or null | Git repository root reported by `git rev-parse --show-toplevel`. |
| `branch` | string or null | Current branch or `HEAD` when detached. |
| `commit` | string or null | Full commit SHA from `git rev-parse HEAD`. |
| `short_commit` | string or null | First 12 characters of `commit`. |
| `describe` | string or null | Output from `git describe --tags --always --dirty`. |
| `dirty` | boolean or null | Whether tracked or untracked paths were reported by `git status --porcelain=v1 --branch`. |
| `status_counts` | object | Worktree status counts by category. |
| `status_sample` | array of strings | Up to 25 porcelain status sample lines. |
| `status_sample_truncated` | boolean | Whether additional dirty paths were omitted from `status_sample`. |
| `error` | string or null | Git metadata warning, if a command failed. |

The `status_counts` object contains `paths`, `staged`, `unstaged`, `untracked`, `ignored`, `modified`, `added`, `deleted`, `renamed`, `copied`, and `conflicted`.

## `toolchain`

Each item in `toolchain` has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `name` | string | Tool name, such as `rustc`, `cargo`, or `cargo-nextest`. |
| `command` | string | Metadata command that was executed. |
| `available` | boolean | Whether the executable was found on `PATH`. |
| `exit_code` | integer or null | Command exit code, if the command ran. |
| `stdout` | string | Captured standard output. |
| `stderr` | string | Captured standard error or availability warning. |
| `timed_out` | boolean | Whether the metadata command exceeded the timeout. |

The generator only executes version or metadata commands. It does not execute validation gates.

## `checks`

Each item in `checks` has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `check_id` | string | Stable check identifier. |
| `status` | string | One of `pass`, `fail`, `skipped`, `gap`, or `partial`. |
| `command` | string | Exact command associated with the check. The generator records this string and does not execute it. |
| `source` | string | Source of the check record, such as `argument:1`, `argument:json`, a JSON file path, or an auto-detected script path. |
| `category` | string | Caller-defined category, or `auto-detected-local-tooling` for detected scripts. |
| `artifact_path` | string | Retained artifact path, if known. Empty when no artifact is attached. |
| `residual_risk` | string | Remaining risk, missing evidence, skipped scope, or `None recorded`. |
| `follow_up` | string | Issue, decision, PR, or release checklist reference. |
| `notes` | string | Additional bounded context. |

Supplied checks may be passed as repeatable `--check` arguments with semicolon-separated `key=value` pairs. Supported field aliases are:

| Alias | Canonical field |
| --- | --- |
| `id` | `check_id` |
| `artifact` | `artifact_path` |
| `risk` | `residual_risk` |
| `follow-up` | `follow_up` |

## Auto-detected Checks

Unless `--no-auto-detect` is used, the generator scans `tools/testing/*.py` and adds one skipped check for each existing local testing helper except itself.

Auto-detected checks are intentionally `skipped` because the generator does not run them. To turn an auto-detected command into release evidence, run the command separately, retain its transcript or artifact, and pass the result back as a supplied check.

## `summary`

The `summary` object has these fields.

| Field | Type | Description |
| --- | --- | --- |
| `total` | integer | Number of check records. |
| `pass` | integer | Number of passing check records supplied by the caller. |
| `fail` | integer | Number of failing check records supplied by the caller. |
| `skipped` | integer | Number of skipped check records. Auto-detected scripts are skipped by default. |
| `gap` | integer | Number of release gap records. |
| `partial` | integer | Number of partial check records. |
| `has_failed_checks` | boolean | Whether any check status is `fail`. |
| `has_release_gaps` | boolean | Whether any check status is `gap`. |
| `release_readiness_claim` | boolean | Always `false`. |

## Validation

Validate Python syntax:

```powershell
python -m py_compile tools/testing/release_evidence.py
```

Run the read-only JSON report:

```powershell
python -B tools/testing/release_evidence.py --json
```

Check ASCII and trailing whitespace on the release evidence files:

```powershell
rg -n "[^\x00-\x7F]" tools/testing/release_evidence.py tools/testing/release_evidence_schema.md documentations/testing/release-evidence-template.md documentations/testing/index.md
rg -n "[ \t]$" tools/testing/release_evidence.py tools/testing/release_evidence_schema.md documentations/testing/release-evidence-template.md documentations/testing/index.md
```

Validate documentation links after updates with a local Markdown link check that resolves repository-relative paths and same-directory links.

## Troubleshooting

If `git.available` is `false`, install Git or run the generator in an environment where Git is on `PATH`. The report remains usable for supplied check capture, but commit and branch metadata are incomplete.

If `git.dirty` is `true`, do not treat the report as clean release-candidate evidence unless the dirty paths are intentionally part of the release packet and reviewed by the release owner.

If a toolchain entry is missing, install only the tool required for the release gate you intend to run. Missing optional tools do not make the generator fail.

If an auto-detected check appears as `skipped`, that is expected. Run the command separately and pass a supplied `pass`, `fail`, `partial`, or `gap` result when the command is relevant to the release scope.

If `--strict` exits with status 1, inspect supplied `fail` and `gap` records. Strict mode does not approve readiness when it exits with status 0.

## References

- `AGENTS.md`
- `tools/testing/release_evidence.py`
- `tools/testing/README.md`
- `tools/testing/validation_manifest.py`
- `tools/testing/validation_manifest_schema.md`
- `documentations/testing/index.md`
- `documentations/testing/release-evidence-template.md`
- `documentations/testing/ci-release-gate-evidence.md`
- `documentations/testing/step-11-validation-matrix.md`
