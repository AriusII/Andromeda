use super::helpers::metadata_table;
use super::*;

#[test]
fn victim_construction_is_deterministic() {
    let victim = DeadlockVictim::from_cycle(
        vec![
            TransactionId::new(5),
            TransactionId::new(2),
            TransactionId::new(9),
            TransactionId::new(5),
        ],
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();

    assert_eq!(victim.tx_id, TransactionId::new(9));
    assert_eq!(
        victim.cycle_participants,
        vec![
            TransactionId::new(2),
            TransactionId::new(5),
            TransactionId::new(9),
        ]
    );

    let oldest = DeadlockVictim::from_cycle(
        vec![TransactionId::new(5), TransactionId::new(2)],
        DeadlockVictimPolicy::OldestTransactionId,
    )
    .unwrap();
    assert_eq!(oldest.tx_id, TransactionId::new(2));

    let metadata = metadata_table(&[(5, 1), (2, 3)]);
    let youngest = DeadlockVictim::from_cycle_with_transaction_metadata(
        vec![TransactionId::new(5), TransactionId::new(2)],
        DeadlockVictimPolicy::YoungestTransactionStartOrder,
        &metadata,
    )
    .unwrap();
    assert_eq!(youngest.tx_id, TransactionId::new(2));
}

#[test]
fn default_policy_is_valid_and_timeout_is_bounded() {
    assert!(DeadlockPolicy::default().validate().is_ok());
    assert_eq!(DEFAULT_DEADLOCK_TIMEOUT, Duration::from_millis(500));
    assert_eq!(
        DeadlockPolicy::default().detection_timeout,
        Duration::from_millis(500)
    );
    assert!(
        DeadlockPolicy::new(
            Duration::from_millis(500),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .is_ok()
    );
    assert!(
        DeadlockPolicy::new(Duration::ZERO, DeadlockVictimPolicy::YoungestTransactionId).is_err()
    );
    assert!(
        DeadlockPolicy::new(
            MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
            DeadlockVictimPolicy::YoungestTransactionId,
        )
        .is_err()
    );
}

#[test]
fn deadline_rejects_zero_and_too_large_timeouts() {
    let clock = ManualDeadlockClock::new();

    assert!(DeadlockDetectionDeadline::new(clock.now(), Duration::ZERO).is_err());
    assert!(DeadlockDetectionDeadline::new(
        clock.now(),
        MAX_DEADLOCK_TIMEOUT + Duration::from_nanos(1),
    )
    .is_err());
}

#[test]
fn manual_deadlock_clock_is_deterministic() {
    let mut clock = ManualDeadlockClock::at(Duration::from_secs(2));
    let policy = DeadlockPolicy::new(
        Duration::from_millis(500),
        DeadlockVictimPolicy::YoungestTransactionId,
    )
    .unwrap();
    let deadline = DeadlockDetectionDeadline::from_policy(policy, &clock).unwrap();

    assert_eq!(
        deadline.started_at().elapsed_since_clock_start(),
        Duration::from_secs(2)
    );
    assert!(!deadline.is_expired(&clock));

    clock.advance(Duration::from_millis(499)).unwrap();
    assert!(!deadline.is_expired(&clock));

    clock.advance(Duration::from_millis(1)).unwrap();
    assert!(deadline.is_expired(&clock));
}
