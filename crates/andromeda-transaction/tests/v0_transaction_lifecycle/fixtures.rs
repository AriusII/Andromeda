use andromeda_transaction_log::Lsn;

pub(crate) fn durable_status_lsn() -> Lsn {
    Lsn::new(1)
}
