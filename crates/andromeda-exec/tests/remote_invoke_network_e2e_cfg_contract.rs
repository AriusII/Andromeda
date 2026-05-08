use std::fs;
use std::path::PathBuf;

fn crate_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn test_target_block<'a>(manifest: &'a str, test_name: &str) -> &'a str {
    manifest
        .split("[[test]]")
        .skip(1)
        .find(|block| {
            block
                .lines()
                .any(|line| line.trim() == format!("name = \"{test_name}\""))
        })
        .unwrap_or_else(|| panic!("missing [[test]] target for {test_name}"))
}

#[test]
fn remote_invoke_network_e2e_has_real_runtime_quinn_feature_gate() {
    let manifest = crate_file("Cargo.toml");

    assert!(
        manifest.contains(
            "[features]\ndefault = []\nruntime-quinn = [\"andromeda-quic/runtime-quinn\"]"
        ),
        "andromeda-exec must expose a non-default runtime-quinn feature that enables andromeda-quic/runtime-quinn"
    );

    let target = test_target_block(&manifest, "remote_invoke_network_e2e");
    assert!(
        target
            .lines()
            .any(|line| line.trim() == "path = \"tests/remote_invoke_network_e2e.rs\""),
        "remote_invoke_network_e2e must stay bound to its integration-test entry point"
    );
    assert!(
        target
            .lines()
            .any(|line| line.trim() == "required-features = [\"runtime-quinn\"]"),
        "remote_invoke_network_e2e must not compile as an empty test target when runtime-quinn is omitted"
    );
}

#[test]
fn remote_invoke_network_e2e_contains_runtime_quinn_canary() {
    let source = crate_file("tests/remote_invoke_network_e2e.rs");

    assert!(
        source.contains("#![cfg(feature = \"runtime-quinn\")]"),
        "remote network invocation tests must remain cfg-gated on runtime-quinn"
    );
    assert!(
        source.contains("fn runtime_quinn_feature_discovers_remote_invoke_network_e2e_tests()"),
        "remote network invocation tests need a discoverable runtime-quinn canary"
    );
    assert!(
        source.contains("andromeda_quic::QuicClientTransport")
            && source.contains("andromeda_quic::QuicServerTransport"),
        "the canary must prove that andromeda-quic transport traits are available"
    );
    assert!(
        source.contains("#[cfg(andromeda_remote_network_e2e)]"),
        "legacy raw Quinn network scenarios must remain explicitly opt-in until they use the typed transport API"
    );
}
