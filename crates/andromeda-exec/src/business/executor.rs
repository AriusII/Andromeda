mod contracts;
mod mvcc_store;
mod reserve_stock;

pub use mvcc_store::InventoryBusinessMvccStore;
pub use reserve_stock::InventoryReserveStockExecutor;

pub(super) use contracts::{
    validate_inventory_query_stock_contract, validate_inventory_release_stock_contract,
    validate_inventory_reserve_stock_contract,
};

#[cfg(test)]
mod tests {
    use super::super::types::{InventoryStock, ReserveStockCommand};
    use super::*;
    use andromeda_catalog::inventory_reserve_stock_contract;
    use andromeda_core::AndromedaErrorKind;
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
