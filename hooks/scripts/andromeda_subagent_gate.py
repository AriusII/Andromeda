#!/usr/bin/env python3
from andromeda_common import *

event = load_event()
last = str(event.get("last_assistant_message", "")) if isinstance(event, dict) else ""
if last and "handoff" not in last.lower() and any(x in last.lower() for x in ["blocked", "risk", "open question"]):
    warn("Subagent completed with unresolved risks or blockers. Ensure the lead agent receives a handoff note.")
allow()
