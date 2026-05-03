use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_OBJECT_ID, INVENTORY_RESERVE_STOCK_PROCEDURE_ID, ProcedureContract,
};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_srpl::Cardinality;

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
                column_count: result_stream.columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: self.mutation_payload(),
            rows_affected: self.rows_affected,
        })
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
        assert!(
            effect
                .result_evidence()
                .proves_exact_result_and_remaining_stock()
        );
        assert_eq!(procedure.mutation_payload, effect.mutation_payload());
    }
}
