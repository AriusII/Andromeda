use std::collections::BTreeMap;

use andromeda_core::TransactionId;

use super::{DeadlockClock, DeadlockDetectionDeadline, DeadlockResult, WaitForGraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DfsVisitState {
    Visiting,
    Visited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CycleSearchOutcome {
    NoCycle,
    CycleFound(Vec<TransactionId>),
    TimedOut,
}

pub(super) fn find_first_cycle_until<C: DeadlockClock>(
    graph: &WaitForGraph,
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> DeadlockResult<CycleSearchOutcome> {
    let mut states = BTreeMap::<TransactionId, DfsVisitState>::new();
    let mut stack = Vec::<TransactionId>::new();

    for tx_id in sorted_transaction_ids(graph) {
        if deadline.is_expired(clock) {
            return Ok(CycleSearchOutcome::TimedOut);
        }

        if states.contains_key(&tx_id) {
            continue;
        }

        match dfs_cycle_from_until(tx_id, graph, &mut states, &mut stack, deadline, clock)? {
            CycleSearchOutcome::CycleFound(cycle_participants) => {
                return Ok(CycleSearchOutcome::CycleFound(cycle_participants));
            }
            CycleSearchOutcome::TimedOut => return Ok(CycleSearchOutcome::TimedOut),
            CycleSearchOutcome::NoCycle => {}
        }
    }

    Ok(CycleSearchOutcome::NoCycle)
}

fn sorted_transaction_ids(graph: &WaitForGraph) -> Vec<TransactionId> {
    graph.transaction_ids()
}

fn dfs_cycle_from_until<C: DeadlockClock>(
    tx_id: TransactionId,
    graph: &WaitForGraph,
    states: &mut BTreeMap<TransactionId, DfsVisitState>,
    stack: &mut Vec<TransactionId>,
    deadline: DeadlockDetectionDeadline,
    clock: &C,
) -> DeadlockResult<CycleSearchOutcome> {
    if deadline.is_expired(clock) {
        return Ok(CycleSearchOutcome::TimedOut);
    }

    states.insert(tx_id, DfsVisitState::Visiting);
    stack.push(tx_id);

    for blocking_tx in graph.blockers_for(tx_id)? {
        if deadline.is_expired(clock) {
            return Ok(CycleSearchOutcome::TimedOut);
        }

        match states.get(&blocking_tx).copied() {
            Some(DfsVisitState::Visiting) => {
                if let Some(cycle_start) = stack
                    .iter()
                    .position(|stacked_tx| *stacked_tx == blocking_tx)
                {
                    return Ok(CycleSearchOutcome::CycleFound(
                        stack[cycle_start..].to_vec(),
                    ));
                }
            }
            Some(DfsVisitState::Visited) => {}
            None => {
                match dfs_cycle_from_until(blocking_tx, graph, states, stack, deadline, clock)? {
                    CycleSearchOutcome::CycleFound(cycle_participants) => {
                        return Ok(CycleSearchOutcome::CycleFound(cycle_participants));
                    }
                    CycleSearchOutcome::TimedOut => return Ok(CycleSearchOutcome::TimedOut),
                    CycleSearchOutcome::NoCycle => {}
                }
            }
        }
    }

    stack.pop();
    states.insert(tx_id, DfsVisitState::Visited);
    Ok(CycleSearchOutcome::NoCycle)
}
