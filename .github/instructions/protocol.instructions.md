---
applyTo: "crates/andromeda-quic*/**/*.rs,crates/andromeda-rpc*/**/*.rs,**/*.proto"
---

# Protocol instructions

- Do not introduce gRPC; Andromeda uses QUIC plus custom Protobuf protocol contracts.
- Do not introduce JSON as an application protocol surface.
- Preserve ResultStream ordering: metadata and contract context before payload records.
- Keep wire formats explicit, versioned, and validated with codecs/tests.
- Follow ADR-0007 for protocol boundary decisions.
