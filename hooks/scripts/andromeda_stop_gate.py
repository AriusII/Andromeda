#!/usr/bin/env python3
from andromeda_common import *

event = load_event()
text = text_from_event(event)
required_terms = ["summary", "risk", "test", "open"]
if any(term in text.lower() for term in ["architecture", "contract", "wal", "srpl", "quic", "protobuf", "storage"]):
    missing = [term for term in required_terms if term not in text.lower()]
    if len(missing) >= 3:
        warn("Before stopping, verify that the final answer covers summary, risks, tests, and open questions for architecture-level work.")
allow()
