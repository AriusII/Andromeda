use andromeda_types::TransactionId;

use super::{
    DeadlockResult, DeadlockTransactionMetadataTable, DeadlockVictimPolicy,
    MISSING_TRANSACTION_ORDERING_METADATA_REASON, deadlock_error, validate_cycle_participants,
    validate_transaction_id,
};

/// Deterministic deadlock victim description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockVictim {
    pub tx_id: TransactionId,
    pub victim_policy: DeadlockVictimPolicy,
    pub cycle_participants: Vec<TransactionId>,
}

impl DeadlockVictim {
    pub fn new(
        tx_id: TransactionId,
        victim_policy: DeadlockVictimPolicy,
        cycle_participants: Vec<TransactionId>,
    ) -> DeadlockResult<Self> {
        validate_transaction_id(tx_id)?;
        validate_cycle_participants(&cycle_participants)?;

        if !cycle_participants.contains(&tx_id) {
            return Err(deadlock_error(
                "deadlock victim must be a member of cycle participants",
            ));
        }

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_unique(cycle_participants),
        })
    }

    pub fn from_cycle(
        cycle_participants: Vec<TransactionId>,
        victim_policy: DeadlockVictimPolicy,
    ) -> DeadlockResult<Self> {
        validate_cycle_participants(&cycle_participants)?;

        let sorted_participants = sorted_unique(cycle_participants);
        let tx_id = match victim_policy {
            DeadlockVictimPolicy::YoungestTransactionStartOrder => {
                return Err(deadlock_error(
                    "deadlock youngest transaction policy requires ordering metadata",
                ));
            },
            DeadlockVictimPolicy::YoungestTransactionId => sorted_participants
                .last()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
            DeadlockVictimPolicy::OldestTransactionId => sorted_participants
                .first()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
        };

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_participants,
        })
    }

    pub fn from_cycle_with_transaction_metadata(
        cycle_participants: Vec<TransactionId>,
        victim_policy: DeadlockVictimPolicy,
        transaction_metadata: &DeadlockTransactionMetadataTable,
    ) -> DeadlockResult<Self> {
        validate_cycle_participants(&cycle_participants)?;

        let sorted_participants = sorted_unique(cycle_participants);
        let tx_id = match victim_policy {
            DeadlockVictimPolicy::YoungestTransactionStartOrder => {
                select_youngest_by_start_order(&sorted_participants, transaction_metadata)?
            },
            DeadlockVictimPolicy::YoungestTransactionId => sorted_participants
                .last()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
            DeadlockVictimPolicy::OldestTransactionId => sorted_participants
                .first()
                .copied()
                .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))?,
        };

        Ok(Self {
            tx_id,
            victim_policy,
            cycle_participants: sorted_participants,
        })
    }
}

fn sorted_unique(mut tx_ids: Vec<TransactionId>) -> Vec<TransactionId> {
    tx_ids.sort();
    tx_ids.dedup();
    tx_ids
}

fn select_youngest_by_start_order(
    sorted_participants: &[TransactionId],
    transaction_metadata: &DeadlockTransactionMetadataTable,
) -> DeadlockResult<TransactionId> {
    let mut selected: Option<(u64, TransactionId)> = None;

    for tx_id in sorted_participants {
        let Some(metadata) = transaction_metadata.metadata_for(*tx_id)? else {
            return Err(deadlock_error(MISSING_TRANSACTION_ORDERING_METADATA_REASON));
        };
        let candidate = (metadata.start_order, *tx_id);
        if selected.map(|current| candidate > current).unwrap_or(true) {
            selected = Some(candidate);
        }
    }

    selected
        .map(|(_start_order, tx_id)| tx_id)
        .ok_or_else(|| deadlock_error("deadlock cycle must not be empty"))
}
