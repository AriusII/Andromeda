use andromeda_error::AndromedaResult;
use andromeda_procedure_contract::CompletionProtocolVersion;

use crate::ProtocolVersion;

impl CompletionProtocolVersion for ProtocolVersion {
    fn validate_completion_protocol_version(self) -> AndromedaResult<()> {
        self.validate()
    }

    fn completion_protocol_major(self) -> u32 {
        self.major
    }

    fn completion_protocol_minor(self) -> u32 {
        self.minor
    }
}
