# Transaction and Recovery Standard

## Requirements

- Commit visible equals WAL durable.
- RAM is never truth.
- The canonical state is the last valid cold snapshot plus durable WAL.
- Recovery must define REDO, incomplete transaction handling, manifest validation, and corruption response.
- Crash tests are mandatory for transaction semantics.
