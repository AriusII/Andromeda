//! Frame wire encoding and decoding.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};

use super::frame_struct::{FrameBytes, FrameHeader};

pub const FRAME_CODEC_VERSION: u16 = 1;
pub const FRAME_CODEC_HEADER_LEN: usize = 52;
pub const FRAME_CODEC_HEADER_LEN_U16: u16 = FRAME_CODEC_HEADER_LEN as u16;
pub const FRAME_CODEC_CRC_OFFSET: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameCodecEndian {
    NetworkBigEndian,
}

pub const FRAME_CODEC_ENDIAN: FrameCodecEndian = FrameCodecEndian::NetworkBigEndian;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameCodec;

impl FrameCodec {
    pub const HEADER_LEN: usize = FRAME_CODEC_HEADER_LEN;
    pub const ENDIAN: FrameCodecEndian = FRAME_CODEC_ENDIAN;

    pub fn encode(frame: &FrameBytes) -> AndromedaResult<Vec<u8>> {
        let mut header = frame.header;
        header.payload_length = frame.payload.len() as u64;
        header.header_crc = 0;
        header.validate_static_fields()?;
        header.validate_payload_length(frame.payload.len())?;

        let mut encoded = encode_header_with_crc(&header);
        encoded.extend_from_slice(&frame.payload);

        Ok(encoded)
    }

    pub fn decode(bytes: &[u8]) -> AndromedaResult<FrameBytes> {
        let (frame, consumed) = Self::scan_one(bytes)?;
        if consumed != bytes.len() {
            return Err(protocol_error("frame buffer contains trailing bytes"));
        }

        Ok(frame)
    }

    pub fn scan_one(bytes: &[u8]) -> AndromedaResult<(FrameBytes, usize)> {
        if bytes.len() < Self::HEADER_LEN {
            return Err(protocol_error(
                "frame buffer is shorter than the fixed header",
            ));
        }

        let header = decode_header(&bytes[..Self::HEADER_LEN])?;
        header.validate_static_fields()?;

        let payload_length = usize::try_from(header.payload_length)
            .map_err(|_| protocol_error("frame payload length does not fit this platform"))?;
        let frame_length = Self::HEADER_LEN
            .checked_add(payload_length)
            .ok_or_else(|| protocol_error("frame length overflows usize"))?;

        if bytes.len() < frame_length {
            return Err(protocol_error("frame payload is truncated"));
        }

        let payload = bytes[Self::HEADER_LEN..frame_length].to_vec();
        header.validate_payload_length(payload.len())?;

        Ok((FrameBytes { header, payload }, frame_length))
    }

    pub fn scan_all(bytes: &[u8]) -> AndromedaResult<Vec<FrameBytes>> {
        let mut frames = Vec::new();
        let mut offset = 0;

        while offset < bytes.len() {
            let (frame, consumed) = Self::scan_one(&bytes[offset..])?;
            offset = offset
                .checked_add(consumed)
                .ok_or_else(|| protocol_error("frame scan offset overflowed"))?;
            frames.push(frame);
        }

        Ok(frames)
    }

    pub fn header_crc(header: &FrameHeader) -> AndromedaResult<u32> {
        header.validate_static_fields()?;
        let encoded = encode_header_with_crc(&FrameHeader {
            header_crc: 0,
            ..*header
        });
        Ok(u32::from_be_bytes(
            encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4]
                .try_into()
                .expect("fixed CRC slice length"),
        ))
    }
}

fn encode_header_with_crc(header: &FrameHeader) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(FrameCodec::HEADER_LEN);
    write_u16(&mut encoded, FRAME_CODEC_HEADER_LEN_U16);
    write_u16(&mut encoded, FRAME_CODEC_VERSION);
    write_u32(&mut encoded, header.frame_type.wire_code());
    write_u64(&mut encoded, header.request_id.get());
    write_u64(&mut encoded, header.session_id.get());
    write_u64(
        &mut encoded,
        header.tx_id.map(TransactionId::get).unwrap_or_default(),
    );
    encoded.push(u8::from(header.tx_id.is_some()));
    encoded.extend_from_slice(&[0, 0, 0]);
    write_u64(&mut encoded, header.payload_length);
    write_u32(&mut encoded, header.flags);
    write_u32(&mut encoded, 0);

    debug_assert_eq!(encoded.len(), FrameCodec::HEADER_LEN);

    let header_crc = crc32(&encoded);
    encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4]
        .copy_from_slice(&header_crc.to_be_bytes());

    encoded
}

fn decode_header(bytes: &[u8]) -> AndromedaResult<FrameHeader> {
    let header_len = read_u16(bytes, 0);
    if header_len != FRAME_CODEC_HEADER_LEN_U16 {
        return Err(protocol_error("unexpected QUIC frame header length"));
    }

    let version = read_u16(bytes, 2);
    if version != FRAME_CODEC_VERSION {
        return Err(protocol_error("unsupported QUIC frame codec version"));
    }

    if bytes[33..36].iter().any(|byte| *byte != 0) {
        return Err(protocol_error("frame header reserved bytes must be zero"));
    }

    let mut crc_input = [0_u8; FRAME_CODEC_HEADER_LEN];
    crc_input.copy_from_slice(bytes);
    crc_input[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].fill(0);
    let expected_crc = crc32(&crc_input);
    let header_crc = read_u32(bytes, FRAME_CODEC_CRC_OFFSET);
    if header_crc != expected_crc {
        return Err(protocol_error("frame header CRC mismatch"));
    }

    let frame_type = super::frame_code::FrameType::try_from(read_u32(bytes, 4))?;
    let tx_id_present = bytes[32];
    let tx_id = match tx_id_present {
        0 => None,
        1 => Some(TransactionId::new(read_u64(bytes, 24))),
        _ => {
            return Err(protocol_error(
                "frame header transaction-id marker is invalid",
            ));
        }
    };

    let header = FrameHeader {
        frame_type,
        request_id: RequestId::new(read_u64(bytes, 8)),
        session_id: SessionId::new(read_u64(bytes, 16)),
        tx_id,
        payload_length: read_u64(bytes, 36),
        flags: read_u32(bytes, 44),
        header_crc,
    };
    header.validate_header_crc(expected_crc)?;

    Ok(header)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("fixed u16 field length"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed u32 field length"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed u64 field length"),
    )
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameType, MAX_FRAME_PAYLOAD_LENGTH};

    fn frame() -> FrameBytes {
        FrameBytes {
            header: FrameHeader {
                frame_type: FrameType::RpcExecuteRequest,
                request_id: RequestId::new(11),
                session_id: SessionId::new(22),
                tx_id: Some(TransactionId::new(33)),
                payload_length: 4,
                flags: 0,
                header_crc: 0,
            },
            payload: b"body".to_vec(),
        }
    }

    #[test]
    fn codec_round_trips_header_and_payload() {
        let encoded = FrameCodec::encode(&frame()).unwrap();
        let decoded = FrameCodec::decode(&encoded).unwrap();

        assert_eq!(decoded.header.frame_type, FrameType::RpcExecuteRequest);
        assert_eq!(decoded.header.request_id, RequestId::new(11));
        assert_eq!(decoded.header.session_id, SessionId::new(22));
        assert_eq!(decoded.header.tx_id, Some(TransactionId::new(33)));
        assert_eq!(decoded.header.payload_length, 4);
        assert_eq!(decoded.payload, b"body");
        assert_ne!(decoded.header.header_crc, 0);
    }

    #[test]
    fn codec_rejects_corrupt_header_crc() {
        let mut encoded = FrameCodec::encode(&frame()).unwrap();
        encoded[8] ^= 0xAA;

        assert_eq!(
            FrameCodec::decode(&encoded).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    #[test]
    fn codec_rejects_oversized_payload_declaration() {
        let mut encoded = FrameCodec::encode(&frame()).unwrap();
        encoded[36..44].copy_from_slice(&(MAX_FRAME_PAYLOAD_LENGTH + 1).to_be_bytes());
        let crc = crc32_with_encoded_crc_zeroed(&encoded[..FRAME_CODEC_HEADER_LEN]);
        encoded[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4]
            .copy_from_slice(&crc.to_be_bytes());

        assert_eq!(
            FrameCodec::scan_one(&encoded).unwrap_err().kind(),
            AndromedaErrorKind::Protocol
        );
    }

    fn crc32_with_encoded_crc_zeroed(header: &[u8]) -> u32 {
        let mut crc_input = [0_u8; FRAME_CODEC_HEADER_LEN];
        crc_input.copy_from_slice(header);
        crc_input[FRAME_CODEC_CRC_OFFSET..FRAME_CODEC_CRC_OFFSET + 4].fill(0);
        crc32(&crc_input)
    }
}
