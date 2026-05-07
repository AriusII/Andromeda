use andromeda_proto::{CONTRACT_PACKAGE, PROTOCOL_FRAME_ENVELOPE_TYPE, PROTOCOL_PACKAGE};

use super::super::support::{
    CRATE_LOCAL_PROTO_SCHEMAS, PROTOCOL_SCHEMAS, package_declarations, schema_contains,
};

#[test]
fn governed_schemas_declare_package_layout() {
    for schema in CRATE_LOCAL_PROTO_SCHEMAS {
        let declarations = package_declarations(schema.source);
        assert_eq!(
            declarations,
            vec![schema.expected_package.to_string()],
            "{} must declare only package {}",
            schema.relative_path,
            schema.expected_package
        );
    }
    assert!(schema_contains(
        PROTOCOL_SCHEMAS,
        "import \"andromeda/contract/v1/contract.proto\";"
    ));
    assert_eq!(PROTOCOL_PACKAGE, "andromeda.protocol.v1");
    assert_eq!(CONTRACT_PACKAGE, "andromeda.contract.v1");
    assert_eq!(
        PROTOCOL_FRAME_ENVELOPE_TYPE,
        "andromeda.protocol.v1.FrameEnvelope"
    );
}
