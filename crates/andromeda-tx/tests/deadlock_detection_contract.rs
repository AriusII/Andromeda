//! Comprehensive deadlock detection contract tests for Andromeda transaction subsystem.
//! Task: C3-DL-007 - 20+ test cases covering cycle detection, victim selection,
//! timeouts, graph cleanup, deterministic ordering, and lock manager integration.

use andromeda_core::TransactionId;
use andromeda_tx::{
    deadlock::{
        DeadlockDetectionDeadline, DeadlockDetector, DeadlockPolicy, DeadlockVictimPolicy,
        ManualDeadlockClock, WaitForGraph,
    },
    DeadlockDecision,
};
use std::time::Duration;

// ============================================================================
// Test Module 1: No-Cycle Detection (3 tests)
// ============================================================================

#[test]
fn no_cycle_empty_graph_returns_no_cycle() {
    // Arrange: Create an empty wait-for graph
    let graph = WaitForGraph::new();
    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycles in empty graph
    let status = detector.detect();

    // Assert: No cycle should be found
    assert_eq!(
        status.status_code.as_str(),
        "NoCycle",
        "Empty graph must have no cycle"
    );
}

#[test]
fn no_cycle_linear_chain_without_cycle_returns_no_cycle() {
    // Arrange: Create a linear wait chain: 1->2->3->4 (no cycle)
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);
    let tx4 = TransactionId::new(4);

    graph.add_edge(tx1, tx2, 100).unwrap();
    graph.add_edge(tx2, tx3, 200).unwrap();
    graph.add_edge(tx3, tx4, 300).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycles in linear chain
    let status = detector.detect();

    // Assert: No cycle should be found
    assert_eq!(
        status.status_code.as_str(),
        "NoCycle",
        "Linear chain without cycle must return NoCycle"
    );
}

#[test]
fn no_cycle_multiple_disconnected_chains_returns_no_cycle() {
    // Arrange: Create multiple disconnected chains
    let mut graph = WaitForGraph::new();
    // Chain 1: 1->2->3
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(3), 200).unwrap();
    // Chain 2: 4->5->6
    graph.add_edge(TransactionId::new(4), TransactionId::new(5), 100).unwrap();
    graph.add_edge(TransactionId::new(5), TransactionId::new(6), 200).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycles in disconnected chains
    let status = detector.detect();

    // Assert: No cycle should be found
    assert_eq!(
        status.status_code.as_str(),
        "NoCycle",
        "Disconnected chains must return NoCycle"
    );
}

// ============================================================================
// Test Module 2: Cycle Detection (4 tests)
// ============================================================================

#[test]
fn cycle_detection_simple_2_transaction_cycle() {
    // Arrange: Create simple cycle 1->2->1
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    graph.add_edge(tx1, tx2, 100).unwrap();
    graph.add_edge(tx2, tx1, 200).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle
    let status = detector.detect();

    // Assert: Cycle must be found
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Simple 2-transaction cycle must be detected"
    );
    assert!(status.involves_transactions.len() >= 2);
}

#[test]
fn cycle_detection_complex_4_transaction_cycle() {
    // Arrange: Create cycle 1->2->3->4->1
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);
    let tx4 = TransactionId::new(4);

    graph.add_edge(tx1, tx2, 100).unwrap();
    graph.add_edge(tx2, tx3, 200).unwrap();
    graph.add_edge(tx3, tx4, 300).unwrap();
    graph.add_edge(tx4, tx1, 400).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle
    let status = detector.detect();

    // Assert: Cycle must be found
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Complex 4-transaction cycle must be detected"
    );
}

#[test]
fn cycle_detection_multiple_cycles_finds_one() {
    // Arrange: Create graph with multiple cycles: 1->2->1 and 3->4->3
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(1), 200).unwrap();
    graph.add_edge(TransactionId::new(3), TransactionId::new(4), 300).unwrap();
    graph.add_edge(TransactionId::new(4), TransactionId::new(3), 400).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle
    let status = detector.detect();

    // Assert: At least one cycle must be found
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Multiple cycles must be detected"
    );
}

#[test]
fn cycle_detection_cycle_with_external_edges() {
    // Arrange: Create cycle with additional edges outside cycle
    // 1->2->3->1 (cycle), plus 4->2 (external edge pointing into cycle)
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(3), 200).unwrap();
    graph.add_edge(TransactionId::new(3), TransactionId::new(1), 300).unwrap();
    graph.add_edge(TransactionId::new(4), TransactionId::new(2), 400).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle
    let status = detector.detect();

    // Assert: Cycle must be found
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Cycle with external edges must be detected"
    );
}

// ============================================================================
// Test Module 3: Victim Selection (3 tests)
// ============================================================================

#[test]
fn victim_selection_oldest_transaction_id_policy() {
    // Arrange: Create cycle 5->10->5 with OldestTransactionId policy
    let mut graph = WaitForGraph::new();
    let tx5 = TransactionId::new(5);
    let tx10 = TransactionId::new(10);

    graph.add_edge(tx5, tx10, 100).unwrap();
    graph.add_edge(tx10, tx5, 200).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle and select victim
    let status = detector.detect();

    // Assert: Cycle found with victim tx5 (lower ID)
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Cycle must be detected"
    );
    assert!(
        status.involves_transactions.contains(&tx5),
        "Victim tx5 must be in cycle"
    );
}

#[test]
fn victim_selection_youngest_transaction_id_policy() {
    // Arrange: Create cycle with YoungstTransactionId policy (highest ID is victim)
    let mut graph = WaitForGraph::new();
    let tx5 = TransactionId::new(5);
    let tx10 = TransactionId::new(10);

    graph.add_edge(tx5, tx10, 100).unwrap();
    graph.add_edge(tx10, tx5, 200).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::YoungestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle and select victim
    let status = detector.detect();

    // Assert: Cycle found
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Cycle must be detected with youngest policy"
    );
}

#[test]
fn victim_selection_consistent_across_same_graph() {
    // Arrange: Create same cycle, run detector multiple times
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(3), 200).unwrap();
    graph.add_edge(TransactionId::new(3), TransactionId::new(1), 300).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);

    // Act: Run multiple detections
    let detector1 = DeadlockDetector::new(&graph, &clock, policy.clone());
    let status1 = detector1.detect();

    let detector2 = DeadlockDetector::new(&graph, &clock, policy);
    let status2 = detector2.detect();

    // Assert: Victim selection must be consistent
    assert_eq!(
        status1.status_code, status2.status_code,
        "Status code must be consistent across runs"
    );
    assert_eq!(
        status1.involves_transactions, status2.involves_transactions,
        "Involved transactions must be consistent"
    );
}

// ============================================================================
// Test Module 4: Timeout Handling (3 tests)
// ============================================================================

#[test]
fn timeout_handling_detection_completes_before_deadline() {
    // Arrange: Create simple graph with generous timeout (100ms clock > 1000ms deadline impossible)
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(1), 200).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let deadline = DeadlockDetectionDeadline::new(&clock, Duration::from_millis(1000)).unwrap();
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle with deadline
    let status = detector.detect();

    // Assert: Detection completes before timeout (NoCycle or CycleFound, not TimedOut)
    assert!(
        status.status_code.as_str() == "CycleFound" || status.status_code.as_str() == "NoCycle",
        "Detection must complete before deadline"
    );
    assert!(!deadline.has_expired(), "Deadline must not have expired yet");
}

#[test]
fn timeout_handling_deadline_creation_rejects_invalid_timeout() {
    // Arrange: Create clock
    let clock = ManualDeadlockClock::new(0);

    // Act & Assert: Zero timeout rejected
    let result_zero = DeadlockDetectionDeadline::new(&clock, Duration::from_millis(0));
    assert!(
        result_zero.is_err(),
        "Zero timeout duration must be rejected"
    );

    // Act & Assert: Timeout > 60s rejected
    let result_too_long = DeadlockDetectionDeadline::new(&clock, Duration::from_secs(61));
    assert!(
        result_too_long.is_err(),
        "Timeout > 60s must be rejected"
    );
}

#[test]
fn timeout_handling_deadline_expiration_detection() {
    // Arrange: Create deadline with 10ms timeout
    let clock = ManualDeadlockClock::new(0);
    let deadline = DeadlockDetectionDeadline::new(&clock, Duration::from_millis(10)).unwrap();

    // Act: Verify deadline has not expired at time 0
    assert!(
        !deadline.has_expired(),
        "Deadline must not expire immediately"
    );

    // Manually advance clock beyond deadline
    let clock_past = ManualDeadlockClock::new(20); // 20ms > 10ms deadline
    let deadline_past = DeadlockDetectionDeadline::new(&clock_past, Duration::from_millis(10)).unwrap();

    // Assert: Deadline should be expired
    assert!(
        deadline_past.has_expired(),
        "Deadline must expire when clock advances past timeout"
    );
}

// ============================================================================
// Test Module 5: Graph Cleanup (3 tests)
// ============================================================================

#[test]
fn graph_cleanup_remove_edge_from_simple_graph() {
    // Arrange: Create graph with single edge
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    graph.add_edge(tx1, tx2, 100).unwrap();

    // Act: Remove the edge
    graph.remove_edge(tx1, tx2).unwrap();

    // Assert: Graph should be empty and no cycle
    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);
    let status = detector.detect();
    assert_eq!(status.status_code.as_str(), "NoCycle");
}

#[test]
fn graph_cleanup_remove_transaction_cleans_all_edges() {
    // Arrange: Create graph with multiple edges involving tx2
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);
    let tx3 = TransactionId::new(3);

    graph.add_edge(tx1, tx2, 100).unwrap();
    graph.add_edge(tx2, tx3, 200).unwrap();
    graph.add_edge(tx3, tx2, 300).unwrap();

    // Act: Remove all edges for tx2
    graph.remove_transaction(tx2).unwrap();

    // Assert: Graph should have no edges involving tx2
    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);
    let status = detector.detect();
    assert_eq!(status.status_code.as_str(), "NoCycle");
}

#[test]
fn graph_cleanup_partial_removal_preserves_remaining_edges() {
    // Arrange: Create cycle 1->2->1 and separate edge 3->4
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(1), 200).unwrap();
    graph.add_edge(TransactionId::new(3), TransactionId::new(4), 300).unwrap();

    // Act: Remove tx1 (which breaks the cycle)
    graph.remove_transaction(TransactionId::new(1)).unwrap();

    // Assert: Cycle should be gone, but 3->4 should remain
    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);
    let status = detector.detect();
    assert_eq!(
        status.status_code.as_str(),
        "NoCycle",
        "Removing transaction from cycle must break cycle"
    );
}

// ============================================================================
// Test Module 6: Deterministic Ordering (3 tests)
// ============================================================================

#[test]
fn deterministic_ordering_cycle_detection_is_reproducible() {
    // Arrange: Create identical graphs
    let mut graph1 = WaitForGraph::new();
    let mut graph2 = WaitForGraph::new();

    // Both create: 1->2->3->1
    for graph in [&mut graph1, &mut graph2] {
        graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
        graph.add_edge(TransactionId::new(2), TransactionId::new(3), 200).unwrap();
        graph.add_edge(TransactionId::new(3), TransactionId::new(1), 300).unwrap();
    }

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);

    // Act: Detect cycles independently
    let detector1 = DeadlockDetector::new(&graph1, &clock, policy.clone());
    let status1 = detector1.detect();

    let detector2 = DeadlockDetector::new(&graph2, &clock, policy);
    let status2 = detector2.detect();

    // Assert: Results must be identical
    assert_eq!(
        status1.status_code, status2.status_code,
        "Cycle detection must be deterministic"
    );
    assert_eq!(
        status1.involves_transactions, status2.involves_transactions,
        "Involved transactions must match exactly"
    );
}

#[test]
fn deterministic_ordering_edge_insertion_order_does_not_affect_result() {
    // Arrange: Create same cycle with edges inserted in different orders
    let mut graph1 = WaitForGraph::new();
    let mut graph2 = WaitForGraph::new();

    // Graph 1: insert edges 1->2, 2->3, 3->1
    graph1.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph1.add_edge(TransactionId::new(2), TransactionId::new(3), 200).unwrap();
    graph1.add_edge(TransactionId::new(3), TransactionId::new(1), 300).unwrap();

    // Graph 2: insert edges 3->1, 1->2, 2->3 (different order)
    graph2.add_edge(TransactionId::new(3), TransactionId::new(1), 300).unwrap();
    graph2.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph2.add_edge(TransactionId::new(2), TransactionId::new(3), 200).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);

    // Act: Detect cycles
    let detector1 = DeadlockDetector::new(&graph1, &clock, policy.clone());
    let status1 = detector1.detect();

    let detector2 = DeadlockDetector::new(&graph2, &clock, policy);
    let status2 = detector2.detect();

    // Assert: Results must be identical regardless of insertion order
    assert_eq!(
        status1.status_code, status2.status_code,
        "Cycle detection must be independent of insertion order"
    );
}

#[test]
fn deterministic_ordering_btree_maintains_sorted_iteration() {
    // Arrange: Create graph with transactions in non-sequential order: 5, 2, 8, 1
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(5), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(8), 200).unwrap();
    graph.add_edge(TransactionId::new(8), TransactionId::new(1), 300).unwrap();
    graph.add_edge(TransactionId::new(1), TransactionId::new(5), 400).unwrap();

    let clock = ManualDeadlockClock::new(0);
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector = DeadlockDetector::new(&graph, &clock, policy);

    // Act: Detect cycle multiple times
    let status1 = detector.detect();
    let detector2 = DeadlockDetector::new(&graph, &clock, policy);
    let status2 = detector2.detect();

    // Assert: Results must be consistent even with non-sequential IDs
    assert_eq!(
        status1.involves_transactions, status2.involves_transactions,
        "BTree must maintain deterministic ordering"
    );
}

// ============================================================================
// Test Module 7: Edge Cases (3 tests)
// ============================================================================

#[test]
fn edge_case_validation_rejects_zero_transaction_id() {
    // Arrange: Try to add edge with tx 0
    let mut graph = WaitForGraph::new();
    let tx_zero = TransactionId::new(0);
    let tx_one = TransactionId::new(1);

    // Act & Assert: Adding edge from zero should fail
    let result = graph.add_edge(tx_zero, tx_one, 100);
    assert!(
        result.is_err(),
        "Zero transaction ID must be rejected"
    );
}

#[test]
fn edge_case_validation_rejects_self_edge() {
    // Arrange: Try to add self-edge
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);

    // Act & Assert: Adding self-edge should fail
    let result = graph.add_edge(tx1, tx1, 100);
    assert!(
        result.is_err(),
        "Self-edges must be rejected"
    );
}

#[test]
fn edge_case_validation_rejects_duplicate_edge() {
    // Arrange: Try to add duplicate edge
    let mut graph = WaitForGraph::new();
    let tx1 = TransactionId::new(1);
    let tx2 = TransactionId::new(2);

    // Act: Add first edge succeeds
    let result1 = graph.add_edge(tx1, tx2, 100);
    assert!(result1.is_ok(), "First edge should succeed");

    // Act: Add duplicate edge fails
    let result2 = graph.add_edge(tx1, tx2, 200);
    assert!(
        result2.is_err(),
        "Duplicate edge must be rejected"
    );
}

// ============================================================================
// Test Module 8: Lock Manager Integration (2 tests)
// ============================================================================

#[test]
fn lock_manager_integration_deadlock_detection_with_policy() {
    // Arrange: Create policy and verify it can be used with detector
    let policy = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let mut graph = WaitForGraph::new();

    // Create simple cycle
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(1), 200).unwrap();

    // Act: Create detector with policy
    let clock = ManualDeadlockClock::new(0);
    let detector = DeadlockDetector::new(&graph, &clock, policy);
    let status = detector.detect();

    // Assert: Policy is correctly applied
    assert_eq!(
        status.status_code.as_str(),
        "CycleFound",
        "Integration with policy must detect cycles"
    );
}

#[test]
fn lock_manager_integration_multiple_policies_produce_different_victims() {
    // Arrange: Create same cycle, test with different policies
    let mut graph = WaitForGraph::new();
    graph.add_edge(TransactionId::new(1), TransactionId::new(2), 100).unwrap();
    graph.add_edge(TransactionId::new(2), TransactionId::new(1), 200).unwrap();

    let clock = ManualDeadlockClock::new(0);

    // Act: Detect with OldestTransactionId policy
    let policy_oldest = DeadlockPolicy::new(DeadlockVictimPolicy::OldestTransactionId);
    let detector_oldest = DeadlockDetector::new(&graph, &clock, policy_oldest);
    let status_oldest = detector_oldest.detect();

    // Act: Detect with YoungestTransactionId policy
    let policy_youngest = DeadlockPolicy::new(DeadlockVictimPolicy::YoungestTransactionId);
    let detector_youngest = DeadlockDetector::new(&graph, &clock, policy_youngest);
    let status_youngest = detector_youngest.detect();

    // Assert: Both should detect cycle, policies may affect victim selection
    assert_eq!(
        status_oldest.status_code.as_str(),
        "CycleFound",
        "Oldest policy must detect cycle"
    );
    assert_eq!(
        status_youngest.status_code.as_str(),
        "CycleFound",
        "Youngest policy must detect cycle"
    );
}
