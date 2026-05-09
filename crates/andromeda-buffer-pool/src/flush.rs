use andromeda_wal::Lsn;

use crate::WalDurabilityObserver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushBlockedFrameCore {
    page_id: u64,
    first_dirty_lsn: Lsn,
    last_dirty_lsn: Lsn,
    max_durable_lsn: Lsn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlushReadiness {
    Ready,
    Blocked(FlushBlockedFrameCore),
}

impl FlushBlockedFrameCore {
    pub const fn new(
        page_id: u64,
        first_dirty_lsn: Lsn,
        last_dirty_lsn: Lsn,
        max_durable_lsn: Lsn,
    ) -> Self {
        Self {
            page_id,
            first_dirty_lsn,
            last_dirty_lsn,
            max_durable_lsn,
        }
    }

    pub const fn page_id(self) -> u64 {
        self.page_id
    }

    pub const fn first_dirty_lsn(self) -> Lsn {
        self.first_dirty_lsn
    }

    pub const fn last_dirty_lsn(self) -> Lsn {
        self.last_dirty_lsn
    }

    pub const fn max_durable_lsn(self) -> Lsn {
        self.max_durable_lsn
    }

    pub fn lsn_gap(self) -> u64 {
        self.last_dirty_lsn
            .get()
            .saturating_sub(self.max_durable_lsn.get())
    }
}

pub fn classify_flush_candidate(
    page_id: u64,
    first_dirty_lsn: Lsn,
    last_dirty_lsn: Lsn,
    observer: &dyn WalDurabilityObserver,
) -> FlushReadiness {
    let max_durable_lsn = observer.max_durable_lsn();
    if observer.is_durable(last_dirty_lsn) && max_durable_lsn >= last_dirty_lsn {
        FlushReadiness::Ready
    } else {
        FlushReadiness::Blocked(FlushBlockedFrameCore::new(
            page_id,
            first_dirty_lsn,
            last_dirty_lsn,
            max_durable_lsn,
        ))
    }
}
