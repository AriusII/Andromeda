# P02 Pre-Transaction Rejection Matrix

## Objective

Document P02 rejection behavior before transaction allocation or ProductStock mutation for the `Inventory.ReserveStock` vertical.

## Security and Admission Rule

Pre-transaction rejection must fail closed:

- no transaction id is allocated
- no durable LSN is emitted
- no WAL record is appended
- ProductStock is not prepared or published
- security authorization denial records the denied logical permission
- contract/admission rejections carry typed rejection evidence

Certificate identity and logical user permission are separate concerns. This V0 evidence exercises logical permission denial through the invocation context and verifies no certificate-style bypass exists in the demo path.

## Rejection Matrix

| Case | Trigger | Evidence Test | Required Result |
|---|---|---|---|
| Malformed execute frame | Short encoded frame | `failure_rollback_gates::v0_inventory_rejects_malformed_execute_frame_before_product_stock_or_wal` | Protocol error, empty WAL, ProductStock untouched |
| Corrupt payload domain | Invalid V0 payload domain byte | `failure_rollback_gates::v0_inventory_rejects_invalid_payload_domain_before_product_stock_or_wal` | Protocol error, empty WAL, ProductStock untouched |
| Transaction-bearing execute frame | Client supplies tx id on command frame | `failure_rollback_gates::v0_inventory_rejects_transaction_bearing_execute_frame_before_product_stock_or_wal` | Protocol error, empty WAL, ProductStock untouched |
| Contract mismatch | Wrong expected contract hash | `failure_rollback_gates::v0_inventory_rejects_contract_mismatch_before_wal_append` | Contract error, empty WAL |
| Missing contract binding | Request omits expected `ProcedureContractBinding` | `failure_rollback_gates::v0_inventory_rejects_missing_contract_binding_before_product_stock_or_wal` | Contract error, empty WAL, ProductStock untouched |
| Missing permission | Invocation context lacks `Inventory.ReserveStock.Execute` | `failure_rollback_gates::v0_inventory_rejects_missing_permission_before_wal_append` | Security error, empty WAL |
| Observed contract rejection | Wrong expected contract hash on observed path | `failure_rollback_gates::v0_inventory_observed_contract_rejection_emits_pre_transaction_evidence` | ContractRejected trace and pre-transaction transition, no transaction evidence |
| Observed authorization denial | Missing execute permission on observed path | `failure_rollback_gates::v0_inventory_observed_authorization_denial_emits_pre_transaction_evidence` | AuthorizationDenied trace names `Inventory.ReserveStock.Execute`, no transaction evidence |
| Observed admission rejection | Zero invocation id | `failure_rollback_gates::v0_inventory_observed_admission_rejection_emits_pre_transaction_evidence` | ContractRejected trace, no transaction evidence |
| Sink failure on rejection path | Event sink fails before WAL | `failure_rollback_gates::v0_inventory_observed_rejection_surfaces_and_counts_emitter_failure_without_wal` | Internal error surfaces, emitter rejection counted, WAL empty |

## Evidence Command

```powershell
cargo test -p andromeda-inventory-demo --test v0_vertical_e2e --locked -- --nocapture
```

Local result on 2026-05-10: PASS, 25 tests passed.

## Acceptance Decision

The P02 V0 pre-transaction rejection matrix is accepted when the focused vertical command passes and the rejection tests above continue to show fail-closed behavior before WAL append or ProductStock mutation.
