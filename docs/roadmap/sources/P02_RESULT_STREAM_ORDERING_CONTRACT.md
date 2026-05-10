# P02 ResultStream Ordering Contract

## Objective

Specify the V0 ResultStream ordering and payload evidence required by the P02 `Inventory.ReserveStock` durable vertical.

## Ordering Rule

The result stream must emit exactly this sequence for the successful P02 V0 path:

1. `RpcMetadata`
2. `RpcBatch`
3. `RpcCompletion`

No payload frame may appear before metadata. No batch may follow terminal completion.

## V0 Payload Evidence

The V0 inventory demo uses explicit byte layouts for compatibility payloads:

| Frame | Domain Tag | Required Decoded Evidence |
|---|---|---|
| `RpcMetadata` | `andromeda.exec.v0.result-metadata.v1` | stream id `1`, exact row count `1`, column count `1`, exact-one marker `1` |
| `RpcBatch` | `andromeda.exec.v0.inventory-reservation-batch.v1` | product id `42`, quantity `3`, remaining quantity `7`, reserved `true`, rows affected `2` |
| `RpcCompletion` | `andromeda.exec.v0.completion.v1` | rows affected `2`, durable LSN `3`, durable LSN non-zero |

The completion frame is terminal audit evidence: it is valid only when it carries non-zero durable LSN evidence for the committed mutation path.

## Negative Contract

The sequence validator must reject any batch after completion. The local rejection evidence is:

- Test: `result_stream_metadata::v0_inventory_result_stream_rejects_batch_after_terminal_completion`
- Expected kind: `AndromedaErrorKind::Protocol`
- Expected message fragment: `batch must not follow completion`

## Evidence Command

```powershell
cargo test -p andromeda-inventory-demo --test v0_vertical_e2e --locked -- --nocapture
```

Local result on 2026-05-10: PASS, 25 tests passed.

## Acceptance Decision

P02 accepts the V0 ResultStream ordering contract when the focused vertical command passes and the decoded payload test proves metadata, batch, and completion evidence, including non-zero durable LSN in completion.
