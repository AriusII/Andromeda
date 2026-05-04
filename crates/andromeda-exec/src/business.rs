use andromeda_catalog::{
    ProcedureContract, INVENTORY_RESERVE_STOCK_OBJECT_ID, INVENTORY_RESERVE_STOCK_PROCEDURE_ID,
};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_srpl::Cardinality;
use andromeda_tx::{MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable};
use std::collections::BTreeMap;

use crate::{CompletionStatus, InvocationCompletion, LocalProcedure, ResultStreamMetadata};

pub const INVENTORY_RESERVE_STOCK_RESULT_STREAM_ID: u64 = 1;
pub const INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS: u64 = 1;
pub const INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED: u64 = 1;
pub const INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED: u64 = 1;
const RESERVE_STOCK_PAYLOAD_DOMAIN: &[u8] = b"andromeda.business.inventory.reserve-stock.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryStock {
    pub product_id: i64,
    pub available_quantity: i64,
    pub version: u64,
}

impl InventoryStock {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.product_id <= 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory product id must be positive",
            ));
        }

        if self.available_quantity < 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory available quantity must not be negative",
            ));
        }

        if self.version == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory stock version must not be zero",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReserveStockCommand {
    pub product_id: i64,
    pub quantity: i64,
}

impl ReserveStockCommand {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.product_id <= 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "reserve stock product id must be positive",
            ));
        }

        if self.quantity <= 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "reserve stock quantity must be positive",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservationResult {
    pub product_id: i64,
    pub quantity: i64,
    pub remaining_quantity: i64,
    pub reserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReserveStockRejectionEvidence {
    pub product_id: i64,
    pub requested_quantity: i64,
    pub available_quantity: i64,
    pub reason: String,
}

impl InventoryReserveStockRejectionEvidence {
    pub fn has_business_rule_evidence(&self) -> bool {
        self.product_id > 0
            && self.requested_quantity > 0
            && self.available_quantity >= 0
            && !self.reason.trim().is_empty()
    }

    pub fn decision_trace(&self, trace_id: TraceId) -> DecisionTrace {
        DecisionTrace {
            trace_id,
            decision: CriticalDecisionKind::BusinessRuleDecision,
            reason: format!(
                "Inventory.ReserveStock business rule rejected for product {}: requested {}, available {}; {}",
                self.product_id, self.requested_quantity, self.available_quantity, self.reason
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryReserveStockResultEvidence {
    pub product_id: i64,
    pub reserved_quantity: i64,
    pub remaining_quantity: i64,
    pub exact_result_row_count: u64,
    pub stock_rows_affected: u64,
    pub reservation_rows_affected: u64,
}

impl InventoryReserveStockResultEvidence {
    pub const fn rows_affected(self) -> u64 {
        self.stock_rows_affected + self.reservation_rows_affected
    }

    pub const fn proves_exact_result_and_remaining_stock(self) -> bool {
        self.product_id > 0
            && self.reserved_quantity > 0
            && self.remaining_quantity >= 0
            && self.exact_result_row_count == INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS
            && self.stock_rows_affected == INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED
            && self.reservation_rows_affected == INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED
    }

    pub fn matches_committed_completion(&self, completion: &InvocationCompletion) -> bool {
        completion.status == CompletionStatus::Committed
            && completion.rows_affected == Some(self.rows_affected())
            && completion.durable_lsn.is_some_and(|lsn| lsn.get() != 0)
    }

    pub fn decision_trace(self, trace_id: TraceId) -> DecisionTrace {
        DecisionTrace {
            trace_id,
            decision: CriticalDecisionKind::BusinessRuleDecision,
            reason: format!(
                "Inventory.ReserveStock business rule accepted for product {}: reserved {}, remaining {}, exact result rows {}, rows affected {}",
                self.product_id,
                self.reserved_quantity,
                self.remaining_quantity,
                self.exact_result_row_count,
                self.rows_affected()
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveStockEffect {
    pub previous_stock: InventoryStock,
    pub next_stock: InventoryStock,
    pub result: ReservationResult,
    pub rows_affected: u64,
}

impl ReserveStockEffect {
    pub const fn result_evidence(&self) -> InventoryReserveStockResultEvidence {
        InventoryReserveStockResultEvidence {
            product_id: self.result.product_id,
            reserved_quantity: self.result.quantity,
            remaining_quantity: self.result.remaining_quantity,
            exact_result_row_count: INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS,
            stock_rows_affected: INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED,
            reservation_rows_affected: INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED,
        }
    }

    pub fn mutation_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(RESERVE_STOCK_PAYLOAD_DOMAIN.len() + 1 + 48);
        payload.extend_from_slice(RESERVE_STOCK_PAYLOAD_DOMAIN);
        payload.push(0);
        payload.extend_from_slice(&self.previous_stock.product_id.to_le_bytes());
        payload.extend_from_slice(&self.previous_stock.available_quantity.to_le_bytes());
        payload.extend_from_slice(&self.previous_stock.version.to_le_bytes());
        payload.extend_from_slice(&self.next_stock.available_quantity.to_le_bytes());
        payload.extend_from_slice(&self.next_stock.version.to_le_bytes());
        payload.extend_from_slice(&self.result.quantity.to_le_bytes());
        payload
    }

    pub fn to_local_procedure(
        &self,
        contract: &ProcedureContract,
    ) -> AndromedaResult<LocalProcedure> {
        validate_inventory_reserve_stock_contract(contract)?;
        let result_stream = contract.result_streams.first().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "Inventory.ReserveStock contract must declare a result stream",
            )
        })?;

        Ok(LocalProcedure {
            contract: contract.as_ref(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: INVENTORY_RESERVE_STOCK_RESULT_STREAM_ID,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: result_stream.columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: self.mutation_payload(),
            rows_affected: self.rows_affected,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryStockVersionEvidence {
    pub stock_version_id: u64,
    pub product_id: i64,
    pub stock_version: u64,
    pub observed_quantity: i64,
    pub begin_ts: u64,
    pub end_ts: Option<u64>,
    pub creator_tx_id: TransactionId,
    pub deleter_tx_id: Option<TransactionId>,
}

impl InventoryStockVersionEvidence {
    pub fn proves_visible_stock_for(
        self,
        snapshot: &Snapshot,
        command: ReserveStockCommand,
    ) -> bool {
        self.product_id == command.product_id
            && self.stock_version > 0
            && self.observed_quantity >= 0
            && self.begin_ts <= snapshot.timestamp
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryReservation {
    pub reservation_id: u64,
    pub product_id: i64,
    pub quantity: i64,
    pub stock_version: u64,
    pub transaction_id: TransactionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryReserveStockMvccEvidence {
    pub transaction_id: TransactionId,
    pub decision_ts: u64,
    pub snapshot_ts: u64,
    pub read_stock: InventoryStockVersionEvidence,
    pub written_stock: InventoryStockVersionEvidence,
    pub reservation_id: u64,
    pub reservation_version_id: u64,
}

impl InventoryReserveStockMvccEvidence {
    pub fn proves_reserve_stock_write(self, effect: &ReserveStockEffect) -> bool {
        self.transaction_id.get() != 0
            && self.decision_ts != 0
            && self.snapshot_ts != 0
            && self.read_stock.product_id == effect.previous_stock.product_id
            && self.read_stock.stock_version == effect.previous_stock.version
            && self.written_stock.product_id == effect.next_stock.product_id
            && self.written_stock.stock_version == effect.next_stock.version
            && self.written_stock.observed_quantity == effect.next_stock.available_quantity
            && self.written_stock.creator_tx_id == self.transaction_id
            && self.reservation_id != 0
            && self.reservation_version_id != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReserveStockMvccDecision {
    pub effect: ReserveStockEffect,
    pub evidence: InventoryReserveStockMvccEvidence,
    pub reservation: InventoryReservation,
}

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
/// This is a foundation/test store, not a production persistence layer.  It
/// deliberately reuses the transaction crate's MVCC headers, snapshot context,
/// and transaction status table so business decisions can carry explicit
/// read/write version evidence without introducing a SQL or JSON execution
/// surface.
#[derive(Debug, Clone, PartialEq, Eq)]
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
            .record(creator_tx_id, TransactionStatus::Committed)?;
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
            .expect("visible stock version must come from an existing product entry");
        let observed_index = stock_versions
            .iter()
            .position(|version| version.version_id == previous_version_id)
            .expect("visible stock version id must still be present");
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
            .record(transaction_id, TransactionStatus::Committed)
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
            .record(transaction_id, TransactionStatus::RolledBack)
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

fn validate_business_product_id(product_id: i64) -> AndromedaResult<()> {
    if product_id <= 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Execution,
            "inventory product id must be positive",
        ));
    }

    Ok(())
}

fn validate_business_mvcc_timestamp(timestamp: u64, message: &'static str) -> AndromedaResult<()> {
    if timestamp == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

fn validate_business_mvcc_transaction_id(
    transaction_id: TransactionId,
    message: &'static str,
) -> AndromedaResult<()> {
    if transaction_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Transaction,
            message,
        ));
    }

    Ok(())
}

fn validate_inventory_reserve_stock_contract(contract: &ProcedureContract) -> AndromedaResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_catalog::inventory_reserve_stock_contract;

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
        assert!(effect
            .result_evidence()
            .proves_exact_result_and_remaining_stock());
        assert_eq!(procedure.mutation_payload, effect.mutation_payload());
    }
}
