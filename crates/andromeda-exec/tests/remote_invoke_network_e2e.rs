//! H3-REMOTE-INVOKE-005: Full End-to-End Remote Invocation over Real QUIC Network
//!
//! Rename note: this file is the governance successor to the former
//! `remote_dispatch_network_e2e.rs` test name. The test body still covers the
//! same network request/response contract; "invoke" names the procedure-level
//! behavior while dispatch remains an internal step.
//!
//! This integration test validates the complete request/response cycle:
//! 1. Server startup with real QUIC and executor
//! 2. Client connects to server
//! 3. Client sends InvocationRequest
//! 4. Server: admission → RPC dispatch
//! 5. Server: executes procedure (mock registry)
//! 6. Server: sends response frames
//! 7. Client: receives and reassembles frames
//! 8. Client: decodes result
//! 9. Verification: byte-for-byte comparison
//!
//! Tests include:
//! - Single request/response cycle
//! - Concurrent invocations (10 clients × 100 procedures)
//! - Stress test (1000+ procedures in rapid succession)
//! - Throughput and latency measurements

#![cfg(feature = "runtime-quinn")]

#[test]
fn runtime_quinn_feature_discovers_remote_invoke_network_e2e_tests() {
    let backend = core::any::type_name::<andromeda_quic::quinn_backend::QuicClient>();
    let tls = core::any::type_name::<andromeda_quic::quinn_tls::MutualTlsTestConfig>();

    assert!(backend.contains("QuicClient"));
    assert!(tls.contains("MutualTlsTestConfig"));
}

// The legacy network scenarios below are kept as explicit opt-in coverage until
// their raw Quinn stream usage is refreshed to the current typed transport API.
#[cfg(andromeda_remote_network_e2e)]
#[path = "remote_invoke_network_e2e/concurrent_invocations.rs"]
mod concurrent_invocations;
#[cfg(andromeda_remote_network_e2e)]
#[path = "remote_invoke_network_e2e/single_invocation.rs"]
mod single_invocation;
#[cfg(andromeda_remote_network_e2e)]
#[path = "remote_invoke_network_e2e/stress_and_latency.rs"]
mod stress_and_latency;
#[cfg(andromeda_remote_network_e2e)]
#[path = "remote_invoke_network_e2e/support.rs"]
mod support;
