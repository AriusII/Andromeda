use std::collections::BTreeMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_tx::Lsn;
use andromeda_tx::{MvccRowHeader, Snapshot, TransactionStatus, TransactionStatusTable};

use super::super::helpers::{
    validate_business_mvcc_timestamp, validate_business_mvcc_transaction_id,
    validate_business_product_id,
};
use super::super::types::{
    InventoryReservation, InventoryReserveStockMvccDecision, InventoryReserveStockMvccEvidence,
    InventoryStock, InventoryStockVersionEvidence, ReserveStockCommand,
};
use super::reserve_stock::InventoryReserveStockExecutor;

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
        Self::allocate_monotonic_id(
            &mut self.next_version_id,
            "inventory MVCC version id overflow",
        )
    }

    fn allocate_reservation_id(&mut self) -> AndromedaResult<u64> {
        Self::allocate_monotonic_id(
            &mut self.next_reservation_id,
            "inventory reservation id overflow",
        )
    }

    fn allocate_monotonic_id(
        next_id: &mut u64,
        overflow_message: &'static str,
    ) -> AndromedaResult<u64> {
        let id = *next_id;
        *next_id = (*next_id)
            .checked_add(1)
            .ok_or_else(|| AndromedaError::new(AndromedaErrorKind::Execution, overflow_message))?;
        Ok(id)
    }
}
