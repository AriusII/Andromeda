use andromeda_error::AndromedaResult;

use crate::StructuredObjectHeader;
use crate::generated::contract;

pub fn project_generated_structured_object_header(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<StructuredObjectHeader> {
    andromeda_proto_wire::project_generated_structured_object_header(header)
}

pub fn validate_generated_structured_object_header(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_structured_object_header(header)
}
