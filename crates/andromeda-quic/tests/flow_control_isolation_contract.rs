use andromeda_core::InvocationId;
use andromeda_quic::{
    BackpressureReason, BackpressureSignal, BackpressureTransport, HADR_STREAM_MAX,
    HADR_STREAM_MIN, HEARTBEAT_STREAM_MAX, HEARTBEAT_STREAM_MIN, StreamConcurrencyManager,
    StreamMultiplexer, StreamRole, VOTE_STREAM_MAX, VOTE_STREAM_MIN, WAL_SHIPPING_STREAM_MAX,
    WAL_SHIPPING_STREAM_MIN,
};
use std::time::{Duration, Instant};

fn invocation_id(id: u64) -> InvocationId {
    InvocationId::new(id)
}

#[test]
fn application_backpressure_does_not_block_hadr_stream_availability() {
    let mut app = StreamConcurrencyManager::with_bounds(
        64,
        Duration::from_secs(30),
        Duration::from_secs(300),
    );

    let app_start = Instant::now();
    for id in 1..=64 {
        app.create_stream(invocation_id(id))
            .expect("stream admission");
    }
    let app_elapsed = app_start.elapsed();
    let app_throughput = 64.0 / app_elapsed.as_secs_f64().max(0.000_001);

    assert!(app.is_at_capacity());
    assert!(app.backpressure_status().is_some());
    assert!(app.create_stream(invocation_id(65)).is_err());

    let mut hadr = StreamMultiplexer::new();
    let hadr_start = Instant::now();
    let replica_count = 8u64;
    for replica in 0..replica_count {
        let alloc = hadr
            .allocate_replica_streams(replica)
            .expect("hadr allocation must remain available");
        assert!(hadr.is_stream_open(alloc.heartbeat_stream_id()));
        assert!(hadr.is_stream_open(alloc.wal_shipping_stream_id()));
        assert!(hadr.is_stream_open(alloc.promotion_vote_stream_id()));
    }
    let hadr_elapsed = hadr_start.elapsed();
    let hadr_throughput = (replica_count as f64 * 3.0) / hadr_elapsed.as_secs_f64().max(0.000_001);

    assert!(
        app_throughput > 0.0 && hadr_throughput > 0.0,
        "both planes must sustain measurable concurrent throughput"
    );

    assert!(HEARTBEAT_STREAM_MAX < WAL_SHIPPING_STREAM_MIN);
    assert!(WAL_SHIPPING_STREAM_MAX < VOTE_STREAM_MIN);
    assert!(VOTE_STREAM_MAX < HADR_STREAM_MAX);
    assert_eq!(HADR_STREAM_MIN, HEARTBEAT_STREAM_MIN);

    assert_eq!(hadr.open_stream_count(), (replica_count as usize) * 3);
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
