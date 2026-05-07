#!/usr/bin/env python3
from common import emit, event_context, event_deny_pretool, read_payload, tool_command, tool_name

payload = read_payload()
command = tool_command(payload)

if "cargo clean" in command and "--package" not in command:
    emit(event_deny_pretool("Blocked broad cargo clean. It can destroy useful build evidence. Use targeted cleanup or explain why full cleanup is required."))
    raise SystemExit(0)

if "RUSTFLAGS" in command and "target-cpu=native" in command:
    emit(event_deny_pretool("Blocked global target-cpu=native. Andromeda requires runtime feature detection and portable fallback unless deployment is controlled."))
    raise SystemExit(0)

context = "Rust guard passed. Prefer Rust 2024, explicit codecs, typed errors, no unsafe without SAFETY comments, and validation gates."
emit(event_context("PreToolUse", context))
