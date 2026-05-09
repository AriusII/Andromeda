use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_exec::LocalProcedure;
use andromeda_mvcc::Snapshot;
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_procedure_contract::ProcedureContract;
use andromeda_result_stream::{CompletionStatus, InvocationCompletion, ResultStreamMetadata};
use andromeda_srpl_ir::Cardinality;
use andromeda_types::TransactionId;

use super::constants::{
    INVENTORY_QUERY_STOCK_COLUMN_COUNT, INVENTORY_QUERY_STOCK_RESULT_STREAM_ID,
    INVENTORY_RELEASE_STOCK_EXACT_RESULT_ROWS, INVENTORY_RELEASE_STOCK_RESULT_STREAM_ID,
    INVENTORY_RESERVE_STOCK_EXACT_RESULT_ROWS, INVENTORY_RESERVE_STOCK_RESULT_STREAM_ID,
    RELEASE_STOCK_PAYLOAD_DOMAIN, RESERVE_STOCK_PAYLOAD_DOMAIN,
};
use super::executor::{
    validate_inventory_query_stock_contract, validate_inventory_release_stock_contract,
    validate_inventory_reserve_stock_contract,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryReservation {
    pub reservation_id: u64,
    pub product_id: i64,
    pub quantity: i64,
    pub stock_version: u64,
    pub transaction_id: TransactionId,
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
            && self.stock_rows_affected == 1
            && self.reservation_rows_affected == 1
    }

    pub fn matches_committed_completion(&self, completion: &InvocationCompletion) -> bool {
        completion.status == CompletionStatus::Committed
            && completion.rows_affected == Some(self.rows_affected())
            && completion.durable_lsn.is_some_and(|lsn| !lsn.is_zero())
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
            stock_rows_affected: 1,
            reservation_rows_affected: 1,
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
            contract_binding: contract.binding(),
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

/// Command to query stock visibility for a single product.
///
/// This is a read-only command: it carries no mutation intent and does not
/// require a write transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryStockCommand {
    pub product_id: i64,
}

impl QueryStockCommand {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.product_id <= 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "query stock product id must be positive",
            ));
        }
        Ok(())
    }
}

/// Effect produced by a successful `Inventory.QueryStock` read Procedure.
///
/// - `stock`: The visible stock snapshot, or `None` if the product does not
///   exist or has no committed version visible to the reading snapshot.
/// - `rows_returned`: 1 when a row is visible, 0 when the product is absent.
///   This value drives `ResultStreamMetadata::row_count_exact` on the wire.
///
/// ## Read-only contract
///
/// `mutation_payload` is always empty and `rows_affected` is always 0.
/// `LocalProcedure::validate()` allows an empty payload when rows_affected is 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryStockEffect {
    pub stock: Option<InventoryStock>,
    pub rows_returned: u64,
}

impl QueryStockEffect {
    /// Build a "found" effect for a visible stock record.
    pub fn found(stock: InventoryStock) -> AndromedaResult<Self> {
        stock.validate()?;
        Ok(Self {
            stock: Some(stock),
            rows_returned: 1,
        })
    }

    /// Build a "not found" effect when the product has no visible committed version.
    pub fn not_found() -> Self {
        Self {
            stock: None,
            rows_returned: 0,
        }
    }

    /// Convert this effect into a [`LocalProcedure`] bound to a validated
    /// `Inventory.QueryStock` contract.
    ///
    /// The resulting `LocalProcedure` has:
    /// - `mutation_payload = []` (read-only; no WAL record needed)
    /// - `rows_affected = 0`
    /// - `cardinality = OptionalOne` (0 or 1 rows)
    pub fn to_local_procedure(
        &self,
        contract: &ProcedureContract,
    ) -> AndromedaResult<LocalProcedure> {
        validate_inventory_query_stock_contract(contract)?;

        Ok(LocalProcedure {
            contract: contract.as_ref(),
            contract_binding: contract.binding(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: INVENTORY_QUERY_STOCK_RESULT_STREAM_ID,
                row_count_exact: Some(self.rows_returned),
                row_count_max: Some(1),
                column_count: INVENTORY_QUERY_STOCK_COLUMN_COUNT,
                cardinality: Cardinality::OptionalOne,
            },
            // Read-only: no WAL mutation payload.
            mutation_payload: Vec::new(),
            rows_affected: 0,
        })
    }
}

/// Command to release previously reserved stock, restoring available quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseStockCommand {
    pub product_id: i64,
    pub quantity: i64,
}

impl ReleaseStockCommand {
    pub fn validate(self) -> AndromedaResult<()> {
        if self.product_id <= 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "release stock product id must be positive",
            ));
        }
        if self.quantity <= 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "release stock quantity must be positive",
            ));
        }
        Ok(())
    }
}

/// Effect produced by a successful `Inventory.ReleaseStock` write Procedure.
///
/// Models the restoration of previously reserved stock: `previous_stock` is
/// the reduced-quantity version committed by the matching `ReserveStock`
/// invocation; `next_stock` is the restored version after release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseStockEffect {
    pub previous_stock: InventoryStock,
    pub next_stock: InventoryStock,
    pub rows_affected: u64,
}

impl ReleaseStockEffect {
    /// Encode a deterministic mutation payload for WAL durability.
    ///
    /// Layout:
    /// ```text
    /// [domain_tag] [NUL] [prev_product_id:8] [prev_quantity:8] [prev_version:8]
    ///                    [next_quantity:8]    [next_version:8]  [released_qty:8]
    /// ```
    pub fn mutation_payload(&self) -> Vec<u8> {
        let released_qty =
            self.next_stock.available_quantity - self.previous_stock.available_quantity;
        let mut payload = Vec::with_capacity(RELEASE_STOCK_PAYLOAD_DOMAIN.len() + 1 + 48);
        payload.extend_from_slice(RELEASE_STOCK_PAYLOAD_DOMAIN);
        payload.push(0);
        payload.extend_from_slice(&self.previous_stock.product_id.to_le_bytes());
        payload.extend_from_slice(&self.previous_stock.available_quantity.to_le_bytes());
        payload.extend_from_slice(&self.previous_stock.version.to_le_bytes());
        payload.extend_from_slice(&self.next_stock.available_quantity.to_le_bytes());
        payload.extend_from_slice(&self.next_stock.version.to_le_bytes());
        payload.extend_from_slice(&released_qty.to_le_bytes());
        payload
    }

    /// Convert this effect into a [`LocalProcedure`] bound to a validated
    /// `Inventory.ReleaseStock` contract.
    pub fn to_local_procedure(
        &self,
        contract: &ProcedureContract,
    ) -> AndromedaResult<LocalProcedure> {
        validate_inventory_release_stock_contract(contract)?;
        let result_stream = contract.result_streams.first().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "Inventory.ReleaseStock contract must declare a result stream",
            )
        })?;

        Ok(LocalProcedure {
            contract: contract.as_ref(),
            contract_binding: contract.binding(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: INVENTORY_RELEASE_STOCK_RESULT_STREAM_ID,
                row_count_exact: Some(INVENTORY_RELEASE_STOCK_EXACT_RESULT_ROWS),
                row_count_max: Some(INVENTORY_RELEASE_STOCK_EXACT_RESULT_ROWS),
                column_count: result_stream.columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: self.mutation_payload(),
            rows_affected: self.rows_affected,
        })
    }
}
