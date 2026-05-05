---
applyTo: "schemas/proto/**/*.proto"
---
# Protobuf Contract Instructions
- Protobuf is used as a contract language only.
- Do not define gRPC services.
- Do not add `service` declarations.
- Do not add google.api HTTP annotations.
- Preserve backward-compatible field numbering.
- Never reuse deleted field numbers.
- Prefer explicit versioned packages, for example `andromeda.contract.v1`.
