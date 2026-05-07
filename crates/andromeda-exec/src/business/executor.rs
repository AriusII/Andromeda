use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_OBJECT_ID, INVENTORY_RESERVE_STOCK_PROCEDURE_ID, ProcedureContract,
};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_tx::{Lsn, MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable};
use std::collections::BTreeMap;

use super::constants::{
    INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED, INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED,
};
use super::helpers::{
    validate_business_mvcc_timestamp, validate_business_mvcc_transaction_id,
    validate_business_product_id,
};
use super::types::{
    InventoryReservation, InventoryReserveStockMvccDecision, InventoryReserveStockMvccEvidence,
    InventoryReserveStockRejectionEvidence, InventoryStock, InventoryStockVersionEvidence,
    ReservationResult, ReserveStockCommand, ReserveStockEffect,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionedInventoryStock {
    version_id: u64,
    header: MvccRowHeader,
    stock: InventoryStock,
}

impl VersionedInventoryStock {
    fn evidence(self) -> InventoryStockVersionEvidence {
        InventoryStockVersionEvidence {
            stock_version_id: self.version_id,
            product_id: self.stock.product_id,
            stock_version: self.stock.version,
            observed_quantity: self.stock.available_quantity,
            begin_ts: self.header.begin_ts,
            end_ts: self.header.end_ts,
            creator_tx_id: self.header.creator_tx_id,
            deleter_tx_id: self.header.deleter_tx_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionedInventoryReservation {
    version_id: u64,
    header: MvccRowHeader,
    reservation: InventoryReservation,
}

/// Deterministic in-memory typed business state for Inventory procedures.
///
/// This is a foundation/test store, not a production persistence layer. It
/// deliberately reuses the transaction crate's MVCC headers, snapshot context,
/// and transaction status table so business decisions can carry explicit
/// read/write version evidence without introducing a SQL or JSON execution
/// surface.
#[derive(Debug)]
pub struct InventoryBusinessMvccStore {
    stock_versions: BTreeMap<i64, Vec<VersionedInventoryStock>>,
    reservations: BTreeMap<u64, VersionedInventoryReservation>,
    transaction_statuses: TransactionStatusTable,
    next_version_id: u64,
    next_reservation_id: u64,
}

impl Default for InventoryBusinessMvccStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InventoryBusinessMvccStore {
    pub fn new() -> Self {
        Self {
            stock_versions: BTreeMap::new(),
            reservations: BTreeMap::new(),
            transaction_statuses: TransactionStatusTable::new(),
            next_version_id: 1,
            next_reservation_id: 1,
        }
    }

    pub fn transaction_status(&self, transaction_id: TransactionId) -> Option<TransactionStatus> {
        self.transaction_statuses.status(transaction_id)
    }

    pub fn seed_committed_stock(
        &mut self,
        stock: InventoryStock,
        commit_ts: u64,
        creator_tx_id: TransactionId,
    ) -> AndromedaResult<InventoryStockVersionEvidence> {
        stock.validate()?;
        validate_business_mvcc_timestamp(
            commit_ts,
            "inventory stock commit timestamp must not be zero",
        )?;
        validate_business_mvcc_transaction_id(
            creator_tx_id,
            "inventory stock creator transaction id must not be zero",
        )?;

        let version = VersionedInventoryStock {
            version_id: self.allocate_version_id()?,
            header: MvccRowHeader::open_version(commit_ts, creator_tx_id, None)?,
            stock,
        };
        self.transaction_statuses
            .record_committed_after_durable_wal(
                creator_tx_id,
                Lsn::new(commit_ts),
                Lsn::new(commit_ts),
            )?;
        self.stock_versions
            .entry(stock.product_id)
            .or_default()
            .push(version);
        Ok(version.evidence())
    }

    pub fn read_stock(
        &self,
        product_id: i64,
        snapshot: &Snapshot,
    ) -> AndromedaResult<Option<InventoryStockVersionEvidence>> {
        validate_business_product_id(product_id)?;
        snapshot.validate()?;
        self.visible_stock_version(product_id, snapshot)
            .map(|visible| visible.map(VersionedInventoryStock::evidence))
    }

    pub fn read_reservations(
        &self,
        product_id: i64,
        snapshot: &Snapshot,
    ) -> AndromedaResult<Vec<InventoryReservation>> {
        validate_business_product_id(product_id)?;
        snapshot.validate()?;
        let mut visible = Vec::new();
        for versioned in self.reservations.values() {
            if versioned.reservation.product_id == product_id
                && versioned
                    .header
                    .visible_in_snapshot(snapshot, &self.transaction_statuses)?
            {
                visible.push(versioned.reservation);
            }
        }
        Ok(visible)
    }

    pub fn reserve_stock(
        &mut self,
        transaction_id: TransactionId,
        decision_ts: u64,
        snapshot: &Snapshot,
        command: ReserveStockCommand,
    ) -> AndromedaResult<InventoryReserveStockMvccDecision> {
        validate_business_mvcc_transaction_id(
            transaction_id,
            "reserve stock transaction id must not be zero",
        )?;
        validate_business_mvcc_timestamp(
            decision_ts,
            "reserve stock decision timestamp must not be zero",
        )?;
        snapshot.validate()?;
        command.validate()?;

        if !snapshot.is_current_transaction(transaction_id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "reserve stock write snapshot must belong to the writing transaction",
            ));
        }

        let observed = self
            .visible_stock_version(command.product_id, snapshot)?
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Execution,
                    "inventory stock row is not visible for reservation",
                )
            })?;
        self.reject_if_stock_has_conflicting_successor(command.product_id, observed.version_id)?;

        let effect = InventoryReserveStockExecutor::reserve(command, observed.stock)?;
        let previous_version_id = observed.version_id;
        let new_stock_version_id = self.allocate_version_id()?;
        let reservation_version_id = self.allocate_version_id()?;
        let reservation_id = self.allocate_reservation_id()?;
        let closed_observed_header = observed.header.close_version(decision_ts, transaction_id)?;

        let written_stock = VersionedInventoryStock {
            version_id: new_stock_version_id,
            header: MvccRowHeader::open_version(
                decision_ts,
                transaction_id,
                Some(previous_version_id),
            )?,
            stock: effect.next_stock,
        };
        let reservation = InventoryReservation {
            reservation_id,
            product_id: command.product_id,
            quantity: command.quantity,
            stock_version: effect.next_stock.version,
            transaction_id,
        };
        let versioned_reservation = VersionedInventoryReservation {
            version_id: reservation_version_id,
            header: MvccRowHeader::open_version(decision_ts, transaction_id, None)?,
            reservation,
        };
        self.transaction_statuses
            .record(transaction_id, TransactionStatus::InFlight)?;

        let stock_versions = self
            .stock_versions
            .get_mut(&command.product_id)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "visible inventory stock version lost its product entry before write",
                )
            })?;
        let observed_index = stock_versions
            .iter()
            .position(|version| version.version_id == previous_version_id)
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "visible inventory stock version disappeared before write",
                )
            })?;
        stock_versions[observed_index].header = closed_observed_header;
        stock_versions.push(written_stock);
        self.reservations
            .insert(reservation_id, versioned_reservation);

        let evidence = InventoryReserveStockMvccEvidence {
            transaction_id,
            decision_ts,
            snapshot_ts: snapshot.timestamp,
            read_stock: observed.evidence(),
            written_stock: written_stock.evidence(),
            reservation_id,
            reservation_version_id,
        };

        Ok(InventoryReserveStockMvccDecision {
            effect,
            evidence,
            reservation,
        })
    }

    pub fn commit_transaction_after_durable_wal(
        &mut self,
        transaction_id: TransactionId,
        durable_commit_lsn: u64,
    ) -> AndromedaResult<()> {
        validate_business_mvcc_timestamp(
            durable_commit_lsn,
            "reserve stock commit requires non-zero durable WAL LSN before visibility",
        )?;
        self.transaction_statuses
            .record_committed_after_durable_wal(
                transaction_id,
                Lsn::new(durable_commit_lsn),
                Lsn::new(durable_commit_lsn),
            )
    }

    pub fn rollback_transaction_after_durable_wal(
        &mut self,
        transaction_id: TransactionId,
        durable_rollback_lsn: u64,
    ) -> AndromedaResult<()> {
        validate_business_mvcc_timestamp(
            durable_rollback_lsn,
            "reserve stock rollback requires non-zero durable WAL LSN before completion",
        )?;
        self.transaction_statuses
            .record_rolled_back_after_durable_wal(
                transaction_id,
                Lsn::new(durable_rollback_lsn),
                Lsn::new(durable_rollback_lsn),
            )
    }

    fn visible_stock_version(
        &self,
        product_id: i64,
        snapshot: &Snapshot,
    ) -> AndromedaResult<Option<VersionedInventoryStock>> {
        let Some(versions) = self.stock_versions.get(&product_id) else {
            return Ok(None);
        };

        for version in versions.iter().rev() {
            if version
                .header
                .visible_in_snapshot(snapshot, &self.transaction_statuses)?
            {
                return Ok(Some(*version));
            }
        }

        Ok(None)
    }

    fn reject_if_stock_has_conflicting_successor(
        &self,
        product_id: i64,
        observed_version_id: u64,
    ) -> AndromedaResult<()> {
        let Some(versions) = self.stock_versions.get(&product_id) else {
            return Ok(());
        };

        for version in versions {
            if version.version_id <= observed_version_id {
                continue;
            }

            if self
                .transaction_statuses
                .status(version.header.creator_tx_id)
                != Some(TransactionStatus::RolledBack)
            {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "inventory stock update conflict: observed stock version is stale",
                ));
            }
        }

        Ok(())
    }

    fn allocate_version_id(&mut self) -> AndromedaResult<u64> {
        let id = self.next_version_id;
        self.next_version_id = self.next_version_id.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory MVCC version id overflow",
            )
        })?;
        Ok(id)
    }

    fn allocate_reservation_id(&mut self) -> AndromedaResult<u64> {
        let id = self.next_reservation_id;
        self.next_reservation_id = self.next_reservation_id.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory reservation id overflow",
            )
        })?;
        Ok(id)
    }
}

pub struct InventoryReserveStockExecutor;

impl InventoryReserveStockExecutor {
    pub fn reserve(
        command: ReserveStockCommand,
        stock: InventoryStock,
    ) -> AndromedaResult<ReserveStockEffect> {
        command.validate()?;
        stock.validate()?;

        if command.product_id != stock.product_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "reserve stock command product must match inventory stock product",
            ));
        }

        if stock.available_quantity < command.quantity {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "insufficient inventory stock for reservation",
            ));
        }

        let next_version = stock.version.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory stock version overflow during reservation",
            )
        })?;
        let remaining_quantity = stock.available_quantity - command.quantity;
        let next_stock = InventoryStock {
            product_id: stock.product_id,
            available_quantity: remaining_quantity,
            version: next_version,
        };
        let result = ReservationResult {
            product_id: command.product_id,
            quantity: command.quantity,
            remaining_quantity,
            reserved: true,
        };

        Ok(ReserveStockEffect {
            previous_stock: stock,
            next_stock,
            result,
            rows_affected: INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED
                + INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED,
        })
    }

    pub fn rejection_evidence(
        command: ReserveStockCommand,
        stock: InventoryStock,
        reason: impl Into<String>,
    ) -> InventoryReserveStockRejectionEvidence {
        InventoryReserveStockRejectionEvidence {
            product_id: command.product_id,
            requested_quantity: command.quantity,
            available_quantity: stock.available_quantity,
            reason: reason.into(),
        }
    }
}

pub(super) fn validate_inventory_reserve_stock_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    contract.validate_canonical_hash()?;

    if contract.procedure_id != INVENTORY_RESERVE_STOCK_PROCEDURE_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReserveStock executable requires the reserve stock procedure id",
        ));
    }

    if contract.object.object_id != INVENTORY_RESERVE_STOCK_OBJECT_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReserveStock executable requires the reserve stock catalog object id",
        ));
    }

    if contract.result_streams.len() != 1 || !contract.result_streams[0].row_count_exact_required {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReserveStock executable requires one exact result stream",
        ));
    }

    Ok(())
}

/// Validate a contract presented to `Inventory.QueryStock` handlers.
///
/// Checks procedure id, object id, and that a result stream is declared.
/// `row_count_exact_required` is intentionally not enforced here because the
/// `QueryStock` contract declares `OptionalOne` cardinality (0 or 1 rows),
/// meaning the count is not fixed at contract definition time.
pub(super) fn validate_inventory_query_stock_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    use andromeda_catalog::{INVENTORY_QUERY_STOCK_OBJECT_ID, INVENTORY_QUERY_STOCK_PROCEDURE_ID};

    contract.validate_canonical_hash()?;

    if contract.procedure_id != INVENTORY_QUERY_STOCK_PROCEDURE_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.QueryStock executable requires the query stock procedure id",
        ));
    }

    if contract.object.object_id != INVENTORY_QUERY_STOCK_OBJECT_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.QueryStock executable requires the query stock catalog object id",
        ));
    }

    if contract.result_streams.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.QueryStock executable requires a declared result stream",
        ));
    }

    Ok(())
}

/// Validate a contract presented to `Inventory.ReleaseStock` handlers.
pub(super) fn validate_inventory_release_stock_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    use andromeda_catalog::{
        INVENTORY_RELEASE_STOCK_OBJECT_ID, INVENTORY_RELEASE_STOCK_PROCEDURE_ID,
    };

    contract.validate_canonical_hash()?;

    if contract.procedure_id != INVENTORY_RELEASE_STOCK_PROCEDURE_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReleaseStock executable requires the release stock procedure id",
        ));
    }

    if contract.object.object_id != INVENTORY_RELEASE_STOCK_OBJECT_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReleaseStock executable requires the release stock catalog object id",
        ));
    }

    if contract.result_streams.len() != 1 || !contract.result_streams[0].row_count_exact_required {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReleaseStock executable requires one exact result stream",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::inventory_reserve_stock_contract;
    use andromeda_observe::{CriticalDecisionKind, TraceId};

    #[test]
    fn reserve_stock_business_rules_produce_typed_effect() {
        let effect = InventoryReserveStockExecutor::reserve(
            ReserveStockCommand {
                product_id: 42,
                quantity: 3,
            },
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 7,
            },
        )
        .unwrap();

        assert_eq!(effect.next_stock.available_quantity, 7);
        assert_eq!(effect.next_stock.version, 8);
        assert_eq!(effect.result.remaining_quantity, 7);
        assert!(effect.result.reserved);
        assert_eq!(effect.rows_affected, 2);
        assert!(!effect.mutation_payload().is_empty());

        let evidence = effect.result_evidence();
        assert!(evidence.proves_exact_result_and_remaining_stock());
        assert_eq!(evidence.exact_result_row_count, 1);
        assert_eq!(evidence.rows_affected(), effect.rows_affected);

        let trace = evidence.decision_trace(TraceId::new(77));
        assert_eq!(trace.decision, CriticalDecisionKind::BusinessRuleDecision);
        assert!(trace.has_explanation());
        assert!(trace.reason.contains("remaining 7"));
    }

    #[test]
    fn reserve_stock_rejects_invalid_business_cases() {
        for command in [
            ReserveStockCommand {
                product_id: 0,
                quantity: 1,
            },
            ReserveStockCommand {
                product_id: 42,
                quantity: 0,
            },
        ] {
            assert_eq!(
                InventoryReserveStockExecutor::reserve(
                    command,
                    InventoryStock {
                        product_id: 42,
                        available_quantity: 10,
                        version: 7,
                    },
                )
                .unwrap_err()
                .kind(),
                AndromedaErrorKind::Execution
            );
        }

        assert_eq!(
            InventoryReserveStockExecutor::reserve(
                ReserveStockCommand {
                    product_id: 43,
                    quantity: 1,
                },
                InventoryStock {
                    product_id: 42,
                    available_quantity: 10,
                    version: 7,
                },
            )
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Execution
        );

        assert_eq!(
            InventoryReserveStockExecutor::reserve(
                ReserveStockCommand {
                    product_id: 42,
                    quantity: 11,
                },
                InventoryStock {
                    product_id: 42,
                    available_quantity: 10,
                    version: 7,
                },
            )
            .unwrap_err()
            .kind(),
            AndromedaErrorKind::Execution
        );
    }

    #[test]
    fn reserve_stock_rejection_evidence_is_audit_friendly_decision_trace() {
        let command = ReserveStockCommand {
            product_id: 42,
            quantity: 11,
        };
        let stock = InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        };
        let err = InventoryReserveStockExecutor::reserve(command, stock).unwrap_err();

        let evidence = InventoryReserveStockExecutor::rejection_evidence(
            command,
            stock,
            err.message().to_string(),
        );
        let trace = evidence.decision_trace(TraceId::new(78));

        assert!(evidence.has_business_rule_evidence());
        assert_eq!(trace.decision, CriticalDecisionKind::BusinessRuleDecision);
        assert!(trace.has_explanation());
        assert!(trace.reason.contains("requested 11"));
        assert!(trace.reason.contains("available 10"));
        assert!(!trace.reason.contains("payload"));
    }

    #[test]
    fn reserve_stock_effect_builds_local_procedure_from_canonical_contract() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let effect = InventoryReserveStockExecutor::reserve(
            ReserveStockCommand {
                product_id: 42,
                quantity: 3,
            },
            InventoryStock {
                product_id: 42,
                available_quantity: 10,
                version: 7,
            },
        )
        .unwrap();

        let procedure = effect.to_local_procedure(&contract).unwrap();

        assert_eq!(procedure.contract, contract.as_ref());
        assert_eq!(
            procedure.required_permissions,
            contract.required_permissions
        );
        assert_eq!(procedure.result_metadata.row_count_exact, Some(1));
        assert_eq!(procedure.rows_affected, 2);
        assert!(
            effect
                .result_evidence()
                .proves_exact_result_and_remaining_stock()
        );
        assert_eq!(procedure.mutation_payload, effect.mutation_payload());
    }
}
