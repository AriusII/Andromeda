use andromeda_core::{AndromedaError, AndromedaErrorKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PayloadKind {
    Hello = 1,
    Auth = 2,
    ContractRequest = 3,
    ContractResponse = 4,
    RpcExecuteRequest = 5,
    RpcMetadata = 6,
    RpcBatch = 7,
    RpcCompletion = 8,
    Error = 9,
}

impl PayloadKind {
    pub const fn requires_contract_hash(self) -> bool {
        matches!(
            self,
            Self::RpcExecuteRequest | Self::RpcMetadata | Self::RpcBatch | Self::RpcCompletion
        )
    }

    pub const fn metadata_must_precede_payload(self) -> bool {
        matches!(self, Self::RpcBatch)
    }
}

impl TryFrom<u32> for PayloadKind {
    type Error = AndromedaError;

    fn try_from(value: u32) -> Result<Self, AndromedaError> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Auth),
            3 => Ok(Self::ContractRequest),
            4 => Ok(Self::ContractResponse),
            5 => Ok(Self::RpcExecuteRequest),
            6 => Ok(Self::RpcMetadata),
            7 => Ok(Self::RpcBatch),
            8 => Ok(Self::RpcCompletion),
            9 => Ok(Self::Error),
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unknown Protobuf payload kind",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_kind_uses_locked_v0_family_values() {
        assert_eq!(PayloadKind::try_from(1).unwrap(), PayloadKind::Hello);
        assert_eq!(
            PayloadKind::try_from(5).unwrap(),
            PayloadKind::RpcExecuteRequest
        );
        assert_eq!(PayloadKind::try_from(9).unwrap(), PayloadKind::Error);
        assert_eq!(
            PayloadKind::try_from(10).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }
}
