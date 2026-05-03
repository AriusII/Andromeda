#!/usr/bin/env python3
from andromeda_common import *

event = load_event()
text = text_from_event(event)
for pattern, reason in DANGEROUS_SHELL_PATTERNS:
    if pattern.search(text):
        deny("Andromeda tool gate blocked Bash command: " + reason)
allow()
