# Copilot CLI hooks

This directory contains Copilot CLI hook configuration for Andromeda. The hooks log session, prompt, tool, and error events under `.work/copilot-cli/` and guard write or shell tools before execution.

## Policy

The pre-tool guard denies:

- edits/creates/deletes under `docs/` and `.codex/`
- edits to `.github/copilot-instructions.md`
- `Cargo.lock` edits unless the event explicitly marks a lock-update task
- dangerous shell commands such as root removal, recursive `C:\` removal, `format`, `DROP TABLE`, and protected-branch force pushes

All other tool use is allowed silently.

## Install

Use `.github/hooks/hooks.json` as the Copilot CLI hook configuration. Bash hooks call `.github/hooks/scripts/guard-tool-use.sh`; make shell scripts executable before use on Unix-like runners:

```bash
chmod +x .github/hooks/scripts/*.sh
```

PowerShell variants are included for native Windows execution.

## Local tests

Test a denied path:

```bash
printf '%s\n' '{"toolName":"edit","toolArgs":"{\"path\":\"docs/status.md\"}"}' | bash .github/hooks/scripts/guard-tool-use.sh
```

```powershell
'{"toolName":"edit","toolArgs":"{\"path\":\"docs/status.md\"}"}' | powershell -NoProfile -ExecutionPolicy Bypass -File .github\hooks\scripts\guard-tool-use.ps1
```

Allowed inputs exit with no output.

## Logs

Runtime logs are written to:

- `.work/copilot-cli/session.log`
- `.work/copilot-cli/prompts.log`
- `.work/copilot-cli/tool-usage.log`
- `.work/copilot-cli/errors.log`
