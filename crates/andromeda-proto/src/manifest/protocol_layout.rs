use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
}

impl ProtocolLayout {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.descriptor_set_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "protocol layout descriptor set hash must not be zero",
            ));
        }

        if self.frame_envelope_hash.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "protocol layout frame envelope hash must not be zero",
            ));
        }

        if self.descriptor_set_hash == self.frame_envelope_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "protocol layout descriptor set hash must be distinct from frame envelope hash",
            ));
        }

        Ok(())
    }
}
