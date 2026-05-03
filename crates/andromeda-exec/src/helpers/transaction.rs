use andromeda_core::{InvocationId, TransactionId};

pub const fn transaction_id_for_invocation(invocation_id: InvocationId) -> TransactionId {
    TransactionId::new(invocation_id.get())
}
