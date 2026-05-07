use andromeda_core::TransactionId;
use andromeda_storage::{
    ConceptualRedoPlan, DatabaseManifest, Lsn, RecoveryPlan, RedoRecordDecision, StartupMode,
    WalRecord, WalRecordKind,
};

const TEST_DATABASE_ID: u64 = 1;
const TEST_MANIFEST_VERSION: u64 = 1;
const TEST_SNAPSHOT_ID: u64 = 1;
const TEST_MANIFEST_CRC: u32 = 0xcafe_dead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScenarioTx(TransactionId);

impl ScenarioTx {
    pub(crate) const fn new(id: u64) -> Self {
        Self(TransactionId::new(id))
    }

    const fn id(self) -> TransactionId {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpectedDecision {
    lsn: Lsn,
    decision: RedoRecordDecision,
}

impl ExpectedDecision {
    const fn new(lsn: Lsn, decision: RedoRecordDecision) -> Self {
        Self { lsn, decision }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WalRecordSpec {
    kind: WalRecordKind,
    lsn: Lsn,
    previous_lsn: Option<Lsn>,
    transaction_id: Option<TransactionId>,
    payload: &'static [u8],
}

impl WalRecordSpec {
    const fn new(
        kind: WalRecordKind,
        lsn: Lsn,
        previous_lsn: Option<Lsn>,
        transaction_id: Option<TransactionId>,
        payload: &'static [u8],
    ) -> Self {
        Self {
            kind,
            lsn,
            previous_lsn,
            transaction_id,
            payload,
        }
    }

    fn build(self) -> WalRecord {
        WalRecord::from_parts(
            self.kind,
            self.lsn,
            self.previous_lsn,
            self.transaction_id,
            self.payload,
        )
        .expect("test WAL fixture must be valid")
    }
}

pub(crate) const fn lsn(value: u64) -> Lsn {
    Lsn::new(value)
}

pub(crate) const fn tx(id: u64) -> ScenarioTx {
    ScenarioTx::new(id)
}

const fn maybe_lsn(value: Option<u64>) -> Option<Lsn> {
    match value {
        Some(value) => Some(Lsn::new(value)),
        None => None,
    }
}

pub(crate) fn manifest_at_lsn1() -> DatabaseManifest {
    manifest_with_required_wal_start(Lsn::new(1))
}

pub(crate) fn manifest_zero_start() -> DatabaseManifest {
    manifest_with_required_wal_start(Lsn::ZERO)
}

pub(crate) fn manifest_with_required_wal_start(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: TEST_DATABASE_ID,
        manifest_version: TEST_MANIFEST_VERSION,
        snapshot_id: TEST_SNAPSHOT_ID,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: TEST_MANIFEST_CRC,
    }
}

pub(crate) fn tx_begin(tx: ScenarioTx, at_lsn: u64, previous_lsn: Option<u64>) -> WalRecord {
    WalRecordSpec::new(
        WalRecordKind::TxBegin,
        lsn(at_lsn),
        maybe_lsn(previous_lsn),
        Some(tx.id()),
        &[],
    )
    .build()
}

pub(crate) fn tx_commit(tx: ScenarioTx, at_lsn: u64, previous_lsn: u64) -> WalRecord {
    WalRecordSpec::new(
        WalRecordKind::TxCommit,
        lsn(at_lsn),
        Some(lsn(previous_lsn)),
        Some(tx.id()),
        &[],
    )
    .build()
}

pub(crate) fn tx_rollback(tx: ScenarioTx, at_lsn: u64, previous_lsn: u64) -> WalRecord {
    WalRecordSpec::new(
        WalRecordKind::TxRollback,
        lsn(at_lsn),
        Some(lsn(previous_lsn)),
        Some(tx.id()),
        &[],
    )
    .build()
}

pub(crate) fn transactional_redo(
    kind: WalRecordKind,
    tx: ScenarioTx,
    at_lsn: u64,
    previous_lsn: u64,
    payload: &'static [u8],
) -> WalRecord {
    WalRecordSpec::new(
        kind,
        lsn(at_lsn),
        Some(lsn(previous_lsn)),
        Some(tx.id()),
        payload,
    )
    .build()
}

pub(crate) fn non_transactional_redo(
    kind: WalRecordKind,
    at_lsn: u64,
    previous_lsn: Option<u64>,
    payload: &'static [u8],
) -> WalRecord {
    WalRecordSpec::new(kind, lsn(at_lsn), maybe_lsn(previous_lsn), None, payload).build()
}

pub(crate) fn incomplete_single_redo(
    tx: ScenarioTx,
    kind: WalRecordKind,
    payload: &'static [u8],
) -> Vec<WalRecord> {
    vec![
        tx_begin(tx, 1, None),
        transactional_redo(kind, tx, 2, 1, payload),
    ]
}

pub(crate) fn committed_single_redo(
    tx: ScenarioTx,
    kind: WalRecordKind,
    payload: &'static [u8],
) -> Vec<WalRecord> {
    vec![
        tx_begin(tx, 1, None),
        transactional_redo(kind, tx, 2, 1, payload),
        tx_commit(tx, 3, 2),
    ]
}

pub(crate) fn plan_from_manifest(
    manifest: &DatabaseManifest,
    records: &[WalRecord],
) -> ConceptualRedoPlan {
    RecoveryPlan::from_manifest_and_wal(manifest, StartupMode::SafeStart, records)
        .expect("RecoveryPlan::from_manifest_and_wal must succeed")
}

pub(crate) fn plan_at_lsn1(records: &[WalRecord]) -> ConceptualRedoPlan {
    plan_from_manifest(&manifest_at_lsn1(), records)
}

pub(crate) fn plan_at_zero(records: &[WalRecord]) -> ConceptualRedoPlan {
    plan_from_manifest(&manifest_zero_start(), records)
}

pub(crate) fn decision_for(plan: &ConceptualRedoPlan, lsn: Lsn) -> RedoRecordDecision {
    let decision = plan
        .records
        .iter()
        .find(|record| record.lsn == lsn)
        .map(|record| record.decision);

    assert!(
        decision.is_some(),
        "redo plan must contain decision for LSN {:?}",
        lsn
    );
    decision.expect("redo decision presence asserted")
}

pub(crate) fn expect_decision(lsn_value: u64, decision: RedoRecordDecision) -> ExpectedDecision {
    ExpectedDecision::new(lsn(lsn_value), decision)
}

pub(crate) fn assert_decision(
    plan: &ConceptualRedoPlan,
    lsn_value: u64,
    expected: RedoRecordDecision,
) {
    assert_eq!(
        decision_for(plan, lsn(lsn_value)),
        expected,
        "unexpected redo decision at LSN {:?}",
        lsn(lsn_value)
    );
}

pub(crate) fn assert_decisions(plan: &ConceptualRedoPlan, expectations: &[ExpectedDecision]) {
    for expectation in expectations {
        assert_eq!(
            decision_for(plan, expectation.lsn),
            expectation.decision,
            "unexpected redo decision at LSN {:?}",
            expectation.lsn
        );
    }
}
