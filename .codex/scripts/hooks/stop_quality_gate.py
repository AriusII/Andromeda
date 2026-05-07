#!/usr/bin/env python3
import json

message = (
    "Before finalizing: list changed files, validation run, Andromeda invariants touched, "
    "recovery/security/compatibility impact, and residual risks."
)
print(json.dumps({"systemMessage": message}))
