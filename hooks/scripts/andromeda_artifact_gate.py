#!/usr/bin/env python3
from andromeda_common import *

event = load_event()
text = text_from_event(event)
findings = []
for pattern, reason in FORBIDDEN_DESIGN_PATTERNS:
    if pattern.search(text):
        findings.append(reason)

if findings:
    warn("Andromeda artifact gate found policy-sensitive text: " + " ".join(findings))
allow()
