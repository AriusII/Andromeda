# Andromeda Codex Operating Rules

This `.codex` package is designed for Andromeda: a Rust 1.95.0 / Edition 2024 mission-critical SGBDRT + SRPL project using QUIC + custom Protobuf RPC.

## Native protocol constraints

- Use QUIC as transport.
- Use custom Protobuf RPC contracts and frames.
- Do not introduce gRPC as native application protocol.
- Do not introduce JSON as native application payload/contract format.

## Worker artifact policy

All Codex worker plans, analysis reports, mission reports and final consolidations must be written under `.work/codex/<task-slug>/`.

- Analysis workers: `.work/codex/<task-slug>/analysis/*.md`
- Orchestrator plans: `.work/codex/<task-slug>/plans/*.md`
- Write worker mission reports: `.work/codex/<task-slug>/missions/*.md`
- Final consolidations: `.work/codex/<task-slug>/final/*.md`

## Architecture rule

Use a small number of strict agents and a large set of precise skills. Prefer loading a skill over creating or dispatching a new agent.
