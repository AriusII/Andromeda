#!/usr/bin/env python3
from common import CORE_CONTEXT, emit, event_context, read_payload

payload = read_payload()
emit(event_context("SessionStart", CORE_CONTEXT + "\nUse the repository AGENTS.md hierarchy and load precise skills instead of broad generic context."))
