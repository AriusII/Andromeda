#!/usr/bin/env python3
from common import emit, read_payload

payload = read_payload()
message = (
    "Stop quality gate complete. Before finalizing: provide artifact links when files were created, "
    "state validation performed, cite sources for factual claims, and identify unvalidated risks."
)
emit({"systemMessage": message})
