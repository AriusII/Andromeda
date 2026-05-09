use andromeda_core::InvocationId;
use andromeda_quic::{
    BackpressureReason, BackpressureSignal, BackpressureTransport, StreamConcurrencyManager,
    StreamRole,
};
use std::time::Duration;

fn invocation_id(id: u64) -> InvocationId {
    InvocationId::new(id)
}

#[test]
fn application_backpressure_is_scoped_to_application_concurrency_manager() {
    let mut app = StreamConcurrencyManager::with_bounds(
        64,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );

    for id in 1..=64 {
        app.create_stream(invocation_id(id))
            .expect("stream admission");
    }

    assert!(app.is_at_capacity());
    assert!(app.backpressure_status().is_some());
    assert!(app.create_stream(invocation_id(65)).is_err());
    assert_eq!(app.active_stream_count(), 64);
}

#[test]
fn backpressure_is_soft_state_not_application_datagram_surface() {
    let signal = BackpressureSignal {
        reason: BackpressureReason::WalFlushLag,
        request_id: None,
        retry_after_millis: Some(100),
    };

    assert!(
        signal
            .validate_routing(StreamRole::TelemetryDatagram)
            .is_ok()
    );
    assert!(signal.validate_routing(StreamRole::Diagnostic).is_ok());
    assert!(
        signal
            .validate_routing(StreamRole::CommandBidirectional)
            .is_err()
    );
    assert!(
        signal
            .validate_for_transport(BackpressureTransport::TelemetryDatagram { mtu_bytes: 1400 })
            .is_ok()
    );
}
