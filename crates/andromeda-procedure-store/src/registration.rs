/// Outcome of a Procedure Store registration call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureRegistration {
    Inserted,
    AlreadyRegistered,
}
