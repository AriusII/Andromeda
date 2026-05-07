# Review Policy

## Review classes

| Class           | Examples                     | Required reviewer              |
|-----------------|------------------------------|--------------------------------|
| Documentation   | README, concepts, examples   | Documentation agent            |
| Low-risk design | Naming, diagrams             | Domain agent                   |
| Protocol        | QUIC frames, Protobuf        | Protocol and security agents   |
| Durable state   | WAL, storage, catalog        | Transaction and storage agents |
| Security        | IAM, audit, mTLS             | Security agent                 |
| Runtime unsafe  | Rust unsafe, FFI, allocators | Rust safety agent              |

## Output rule

A review must end with one of: `Approve`, `Approve with changes`, `Request changes`, or `Reject`.
