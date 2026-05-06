/// Outcome of a [`ProcedureStore::register`](super::ProcedureStore::register)
/// call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureRegistration {
    Inserted,
    AlreadyRegistered,
}
