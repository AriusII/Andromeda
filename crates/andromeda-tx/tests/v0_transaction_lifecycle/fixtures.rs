use andromeda_tx::Lsn;

pub(crate) fn durable_status_lsn() -> Lsn {
    Lsn::new(1)
}
