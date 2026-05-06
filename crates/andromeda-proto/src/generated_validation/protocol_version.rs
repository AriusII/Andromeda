use andromeda_core::AndromedaResult;

use crate::version::ProtocolVersion as SupportedProtocolVersion;

pub(crate) fn validate_generated_protocol_version(major: u32, minor: u32) -> AndromedaResult<()> {
    SupportedProtocolVersion { major, minor }.validate()
}
