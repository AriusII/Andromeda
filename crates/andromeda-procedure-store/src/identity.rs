use crate::{ProcedureStorePrimitiveError, ProcedureStorePrimitiveResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvocationId(u64);

impl InvocationId {
    pub fn new(value: u64) -> ProcedureStorePrimitiveResult<Self> {
        if value == 0 {
            return Err(ProcedureStorePrimitiveError::ZeroInvocationId);
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcedureId(u64);

impl ProcedureId {
    pub fn new(value: u64) -> ProcedureStorePrimitiveResult<Self> {
        if value == 0 {
            return Err(ProcedureStorePrimitiveError::ZeroProcedureId);
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvocationIdentity {
    pub invocation_id: InvocationId,
    pub procedure_id: ProcedureId,
}

impl InvocationIdentity {
    pub const fn new(invocation_id: InvocationId, procedure_id: ProcedureId) -> Self {
        Self {
            invocation_id,
            procedure_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_identity_rejects_zero_components() {
        assert_eq!(
            InvocationId::new(0).unwrap_err(),
            ProcedureStorePrimitiveError::ZeroInvocationId
        );
        assert_eq!(
            ProcedureId::new(0).unwrap_err(),
            ProcedureStorePrimitiveError::ZeroProcedureId
        );

        let identity =
            InvocationIdentity::new(InvocationId::new(1).unwrap(), ProcedureId::new(2).unwrap());
        assert_eq!(identity.invocation_id.get(), 1);
        assert_eq!(identity.procedure_id.get(), 2);
    }
}
