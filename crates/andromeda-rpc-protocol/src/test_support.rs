use andromeda_types::{RequestId, SessionId};

use crate::frame_code::{FRAME_HEADER_CRC_UNCHECKED, FrameType};
use crate::frame_struct::{FrameBytes, FrameHeader};

pub(crate) fn frame_header(frame_type: FrameType, payload_length: u64) -> FrameHeader {
    FrameHeader {
        frame_type,
        request_id: RequestId::new(1),
        session_id: SessionId::new(2),
        tx_id: None,
        payload_length,
        flags: 0,
        header_crc: FRAME_HEADER_CRC_UNCHECKED,
    }
}

pub(crate) fn frame_bytes(frame_type: FrameType, payload: Vec<u8>) -> FrameBytes {
    FrameBytes {
        header: frame_header(frame_type, payload.len() as u64),
        payload,
    }
}
