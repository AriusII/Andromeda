#!/usr/bin/env python3
from andromeda_common import *

event = load_event()
text = text_from_event(event)
for pattern, reason in FORBIDDEN_DESIGN_PATTERNS:
    if pattern.search(text):
        warn("Andromeda write gate warning: " + reason + " If this is only a rejected alternative or comparison, label it explicitly.")
allow()
