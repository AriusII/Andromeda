# Testing Tooling

## Purpose

This directory contains read-only testing and developer-experience tooling for Andromeda roadmap validation.

Use these scripts to inspect local readiness before running heavier Rust, fuzz, Miri, or release gates. The scripts do not create, update, delete, stage, commit, or clean repository files.

## Scope

This tooling covers:

- Local preflight visibility for worktree state and key validation tools.
- Windows/MSVC linker visibility for Rust gates that target `*-windows-msvc`.
- Local supply-chain tooling visibility for `cargo-nextest`, `cargo-audit`, `cargo-deny`, `cargo-vet`, and `cargo tree`.
- Step 11 roadmap inventory across crate-owned test suites, fuzz targets, test documentation, runbooks, GitHub workflows, Miri evidence, Loom evidence, and fuzz evidence.
- P01 normative specification baseline checks across required `SPEC_*_V0.md` files, sections, rejection criteria, and key tokens.
- Missing-gate reporting for release-readiness planning.

## Non-goals

This tooling does not:

- Replace crate-owned Rust tests, fuzzing, Miri, Loom, crash/recovery, or release gates.
- Treat generated fuzz artifacts, benchmark output, RAM state, GPU output, or temporary files as durable evidence.
- Move tests into the root `tests/` directory.
- Modify repository files or local machine state.

## Prerequisites

- Python 3 with only the standard library.
- Run commands from the repository root.
- Optional validation tools on `PATH` when you want the preflight to report them as available: `git`, `cargo`, `rustc`, `rustfmt`, `cargo-clippy`, `cargo-nextest`, `cargo-audit`, `cargo-deny`, `cargo-fuzz`, `cargo-miri`, and `cargo-vet`.
- On Windows/MSVC targets, `rustc` on `PATH`, an MSVC `link.exe` exposed by the current shell, and Windows SDK libraries such as `kernel32.lib` and `ucrt.lib`.

## Procedure

Run the local testing preflight:

```powershell
python tools/testing/preflight.py
```

Run the Windows/MSVC linker preflight when a Rust gate fails because `link.exe` is missing:

```powershell
python tools/testing/windows_msvc_preflight.py
python tools/testing/windows_msvc_preflight.py --json
```

Run the Step 11 roadmap inventory:

```powershell
python tools/testing/step11_inventory.py
```

Run the P01 normative specification baseline:

```powershell
python -B tools/testing/p01_spec_baseline_check.py
python -B tools/testing/p01_spec_baseline_check.py --strict
```

Run the supply-chain tooling preflight:

```powershell
python tools/testing/supply_chain_preflight.py
```

Emit machine-readable supply-chain evidence:

```powershell
python tools/testing/supply_chain_preflight.py --json
```

Use strict mode only when a calling workflow should fail on missing required tools or inventory gaps:

```powershell
python tools/testing/preflight.py --strict
python tools/testing/windows_msvc_preflight.py --strict
python tools/testing/supply_chain_preflight.py --strict
python tools/testing/step11_inventory.py --strict
python -B tools/testing/p01_spec_baseline_check.py --strict
```

## Validation

For this tooling, use:

```powershell
python tools/testing/preflight.py
python -c "import pathlib, py_compile, tempfile; td=tempfile.TemporaryDirectory(); py_compile.compile('tools/testing/windows_msvc_preflight.py', cfile=str(pathlib.Path(td.name) / 'windows_msvc_preflight.pyc'), doraise=True); td.cleanup()"
python -B tools/testing/windows_msvc_preflight.py --json
python tools/testing/windows_msvc_preflight.py --strict
python tools/testing/supply_chain_preflight.py
python tools/testing/step11_inventory.py
python -B tools/testing/p01_spec_baseline_check.py
python .codex/scripts/validate_codex_tooling.py
```

For Rust changes, use the applicable crate-owned gates from `tests/README.md` and `docs/testing/RELEASE_GATES.md`.

## Troubleshooting

If `preflight.py` reports missing tools, install only the tool needed for the gate you intend to run. Optional tools are reported for visibility and do not fail default preflight mode.

If `windows_msvc_preflight.py` reports `MSVC_LINK_EXE_MISSING`, treat the Rust gate as blocked by the current shell or host image. The script does not install Visual Studio Build Tools, mutate `PATH`, or repair the Windows SDK; it reports the detected Rust target, `link.exe` visibility, Visual Studio indicators, and Windows SDK library evidence so the blocker can be reproduced.

If `supply_chain_preflight.py` reports missing tools, treat the output as local gate visibility. The script does not install tools. Default mode exits successfully for reporting; `--strict` exits with status `1` when any expected supply-chain tool is missing.


If `step11_inventory.py` reports a missing test path, inspect the owning crate first. Update roadmap documentation only after confirming the suite was renamed, moved, or intentionally removed.

If Loom evidence is reported missing, treat it as a planning gap for concurrency-sensitive promotion claims. Do not satisfy that gap with non-Loom unit tests unless the roadmap decision explicitly changes the required evidence.

## References

- `tests/README.md`
- `docs/testing/RELEASE_GATES.md`
- `docs/adr/ADR-0010-SUPPLY_CHAIN_POLICY.md`
- `docs/testing/TEST_STRATEGY.md`
- `fuzz/README.md`
- `tests/fuzzing/targets.toml`
- `fuzz/VALIDATION_MATRIX.md`
- `.github/workflows/06-nightly-deep-validation.yml`
- `.github/workflows/07-fuzzing.yml`
- `.github/workflows/release-gate-chain.yml`
