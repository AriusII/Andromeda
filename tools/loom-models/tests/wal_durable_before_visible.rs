use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::thread::{self, JoinHandle};

const NO_LSN: usize = 0;
const COMMIT_LSN: usize = 7;

#[derive(Debug)]
struct WalVisibilityPublication {
    durable_lsn: AtomicUsize,
    visible_lsn: AtomicUsize,
}

impl WalVisibilityPublication {
    fn new() -> Self {
        Self {
            durable_lsn: AtomicUsize::new(NO_LSN),
            visible_lsn: AtomicUsize::new(NO_LSN),
        }
    }

    fn publish_commit_after_durable_wal(&self, commit_lsn: usize) {
        self.durable_lsn.store(commit_lsn, Ordering::Release);
        thread::yield_now();
        self.visible_lsn.store(commit_lsn, Ordering::Release);
    }

    fn publish_commit_before_durable_wal_for_detection(&self, commit_lsn: usize) {
        self.visible_lsn.store(commit_lsn, Ordering::Release);
        thread::yield_now();
        self.durable_lsn.store(commit_lsn, Ordering::Release);
    }

    fn assert_visible_commit_has_durable_wal(&self) {
        let visible_lsn = self.visible_lsn.load(Ordering::Acquire);

        if visible_lsn == NO_LSN {
            return;
        }

        let durable_lsn = self.durable_lsn.load(Ordering::Acquire);

        assert!(
            durable_lsn >= visible_lsn,
            "visible commit observed before durable WAL: visible_lsn={visible_lsn}, durable_lsn={durable_lsn}"
        );
    }
}

fn join(handle: JoinHandle<()>) {
    if let Err(payload) = handle.join() {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn visible_commit_is_never_observed_before_durable_wal() {
    loom::model(|| {
        let publication = Arc::new(WalVisibilityPublication::new());

        let publisher = {
            let publication = Arc::clone(&publication);

            thread::spawn(move || {
                publication.publish_commit_after_durable_wal(COMMIT_LSN);
            })
        };

        let observer = {
            let publication = Arc::clone(&publication);

            thread::spawn(move || {
                publication.assert_visible_commit_has_durable_wal();
                thread::yield_now();
                publication.assert_visible_commit_has_durable_wal();
            })
        };

        join(publisher);
        join(observer);

        publication.assert_visible_commit_has_durable_wal();
    });
}

#[test]
#[should_panic(expected = "visible commit observed before durable WAL")]
fn broken_publication_order_is_detected_by_loom() {
    loom::model(|| {
        let publication = Arc::new(WalVisibilityPublication::new());

        let publisher = {
            let publication = Arc::clone(&publication);

            thread::spawn(move || {
                publication.publish_commit_before_durable_wal_for_detection(COMMIT_LSN);
            })
        };

        let observer = {
            let publication = Arc::clone(&publication);

            thread::spawn(move || {
                publication.assert_visible_commit_has_durable_wal();
                thread::yield_now();
                publication.assert_visible_commit_has_durable_wal();
            })
        };

        join(publisher);
        join(observer);
    });
}
