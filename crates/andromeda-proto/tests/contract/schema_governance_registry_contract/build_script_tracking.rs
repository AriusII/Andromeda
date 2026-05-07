use super::super::support::BUILD_SCRIPT;

#[test]
fn build_script_tracks_proto_directories_for_new_schema_discovery() {
    for required_snippet in [
        "collect_proto_directories(&proto_root)?",
        "for proto_directory in &proto_directories",
        "cargo:rerun-if-changed={}",
        "lower.contains(\"grpc\")",
        "lower.contains(\"json\")",
        "lower.contains(\"sql\")",
    ] {
        assert!(
            BUILD_SCRIPT.contains(required_snippet),
            "build.rs must preserve deterministic directory rerun tracking snippet: {required_snippet}"
        );
    }
}
