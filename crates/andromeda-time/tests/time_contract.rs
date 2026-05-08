use andromeda_error::AndromedaErrorKind;
use andromeda_time::{Clock, EngineTimestamp, ManualClock, SystemClock};

#[derive(Debug, Clone, Copy)]
struct FixedClock {
    timestamp: EngineTimestamp,
}

impl FixedClock {
    const fn new(timestamp: EngineTimestamp) -> Self {
        Self { timestamp }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> EngineTimestamp {
        self.timestamp
    }
}

fn observe<C: Clock>(clock: &C) -> EngineTimestamp {
    clock.now()
}

#[test]
fn clock_trait_returns_typed_engine_timestamps() {
    let fixed = FixedClock::new(EngineTimestamp::from_unix_millis(12_345));
    let manual = ManualClock::from_unix_millis(54_321);

    assert_eq!(observe(&fixed), EngineTimestamp::from_unix_millis(12_345));
    assert_eq!(observe(&manual), EngineTimestamp::from_unix_millis(54_321));
}

#[test]
fn system_clock_observation_uses_explicit_timestamp_type() {
    let observed = SystemClock.now();
    let encoded = observed.to_unix_millis_le_bytes();

    assert_eq!(
        EngineTimestamp::try_from_unix_millis_le_slice(&encoded),
        Ok(observed)
    );
}

#[test]
fn unix_millis_encoding_is_fixed_width_little_endian() {
    let unix_millis = 0x0102_0304_0506_0708_u64;
    let timestamp = EngineTimestamp::from_unix_millis(unix_millis);

    assert_eq!(EngineTimestamp::UNIX_MILLIS_LE_BYTE_LEN, 8);
    assert_eq!(
        timestamp.to_unix_millis_le_bytes(),
        [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
    );
    assert_eq!(
        EngineTimestamp::from_unix_millis_le_bytes(timestamp.to_unix_millis_le_bytes()),
        timestamp
    );
}

#[test]
fn unix_millis_decoder_rejects_implicit_lengths() {
    let too_short = [0_u8; EngineTimestamp::UNIX_MILLIS_LE_BYTE_LEN - 1];
    let too_long = [0_u8; EngineTimestamp::UNIX_MILLIS_LE_BYTE_LEN + 1];

    for bytes in [&[][..], &too_short[..], &too_long[..]] {
        let Err(error) = EngineTimestamp::try_from_unix_millis_le_slice(bytes) else {
            panic!("non-canonical timestamp byte length should be rejected");
        };
        assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
    }
}
