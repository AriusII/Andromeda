pub use std::collections::HashMap;
pub use std::net::SocketAddr;
pub use std::sync::Arc;
pub use std::sync::OnceLock;
pub use std::sync::atomic::{AtomicU64, Ordering};
pub use std::time::Instant;

pub use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId,
};
pub use andromeda_quic::frame::FrameType;
pub use andromeda_quic::{FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader};
pub use andromeda_quic_runtime_quinn::{
    quinn_backend::{QuicClient, QuicServer},
    quinn_tls::MutualTlsTestConfig,
};
pub use tokio::sync::RwLock;
use tokio::task::JoinHandle;
pub use tokio::time::{Duration, timeout};

static TEST_TLS: OnceLock<MutualTlsTestConfig> = OnceLock::new();

fn test_tls() -> AndromedaResult<&'static MutualTlsTestConfig> {
    TEST_TLS.get_or_try_init(|| MutualTlsTestConfig::ephemeral(vec!["localhost".to_string()]))
}

fn transport_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Transport, message.into())
}

pub(crate) async fn with_timeout<T>(
    duration: Duration,
    future: impl std::future::Future<Output = AndromedaResult<T>>,
) -> AndromedaResult<T> {
    timeout(duration, future)
        .await
        .map_err(|_| transport_error("operation timed out"))
}

pub(crate) async fn join_with_timeout<T>(
    duration: Duration,
    handle: JoinHandle<AndromedaResult<T>>,
) -> AndromedaResult<T> {
    timeout(duration, handle)
        .await
        .map_err(|_| transport_error("join task timed out"))?
        .map_err(|error| transport_error(format!("task join failed: {error}")))?
}

// Mock Procedure Registry

/// Mock procedure for testing.
#[derive(Debug, Clone)]
pub(crate) struct MockProcedure {
    output_rows: u64,
    row_data: Vec<u8>,
}

/// Mock registry with predefined procedures.
#[derive(Debug, Clone)]
pub(crate) struct MockRegistry {
    procedures: Arc<RwLock<HashMap<String, MockProcedure>>>,
}

impl MockRegistry {
    /// Creates a registry with test procedures.
    pub(crate) fn new() -> Self {
        let mut procedures = HashMap::new();

        // ReserveStock procedure: reserves inventory
        procedures.insert(
            "inventory.ReserveStock".to_string(),
            MockProcedure {
                output_rows: 1,
                row_data: vec![0x00, 0x01, 0x02, 0x03], // Mock row data
            },
        );

        // QueryStock procedure: queries inventory
        procedures.insert(
            "inventory.QueryStock".to_string(),
            MockProcedure {
                output_rows: 10,
                row_data: vec![0x04, 0x05, 0x06, 0x07],
            },
        );

        // System procedure: system information
        procedures.insert(
            "system.version".to_string(),
            MockProcedure {
                output_rows: 1,
                row_data: vec![0x01, 0x00, 0x00, 0x00], // Version 1
            },
        );

        // Catalog procedure: table metadata
        procedures.insert(
            "catalog.tables".to_string(),
            MockProcedure {
                output_rows: 50,
                row_data: vec![0x32], // 50 tables
            },
        );

        Self {
            procedures: Arc::new(RwLock::new(procedures)),
        }
    }

    /// Executes a procedure and returns response frames.
    pub(crate) async fn execute(
        &self,
        procedure_name: &str,
        request_id: RequestId,
        session_id: SessionId,
    ) -> AndromedaResult<Vec<FrameBytes>> {
        let procedures = self.procedures.read().await;

        if let Some(proc) = procedures.get(procedure_name) {
            let mut frames = vec![];

            // 1. Metadata frame
            let row_count_bytes = proc.output_rows.to_le_bytes();
            let metadata_payload = vec![
                0x01, // Metadata policy
                row_count_bytes[0],
                row_count_bytes[1],
                row_count_bytes[2],
                row_count_bytes[3],
            ];

            frames.push(make_response_frame(
                FrameType::RpcMetadata,
                request_id,
                session_id,
                metadata_payload,
            ));

            // 2. Data batch frames (chunked)
            let chunk_size = 256;
            for chunk in proc.row_data.chunks(chunk_size) {
                frames.push(make_response_frame(
                    FrameType::RpcBatch,
                    request_id,
                    session_id,
                    chunk.to_vec(),
                ));
            }

            // 3. Completion frame
            frames.push(make_response_frame(
                FrameType::RpcCompletion,
                request_id,
                session_id,
                Vec::new(),
            ));

            Ok(frames)
        } else {
            Err(andromeda_core::AndromedaError::new(
                AndromedaErrorKind::Execution,
                format!("procedure '{}' not found", procedure_name),
            ))
        }
    }
}

// Helper Functions

pub(crate) fn make_response_frame(
    frame_type: FrameType,
    request_id: RequestId,
    session_id: SessionId,
    payload: Vec<u8>,
) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type,
            request_id,
            session_id,
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

pub(crate) fn create_test_server_tls() -> AndromedaResult<&'static MutualTlsTestConfig> {
    test_tls()
}

pub(crate) fn create_test_client_tls() -> AndromedaResult<&'static MutualTlsTestConfig> {
    test_tls()
}

pub(crate) fn encode_frame(frame: &FrameBytes) -> AndromedaResult<Vec<u8>> {
    FrameCodec::encode(frame)
}

pub(crate) fn decode_response_frames(bytes: &[u8]) -> AndromedaResult<Vec<FrameBytes>> {
    FrameCodec::scan_all(bytes)
}

pub(crate) fn assert_standard_response_sequence(
    frames: &[FrameBytes],
    request_id: RequestId,
    session_id: SessionId,
) -> AndromedaResult<()> {
    if frames.len() < 3 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!(
                "response must contain metadata, batch, and completion frames; got {} frame(s)",
                frames.len()
            ),
        ));
    }

    let metadata = &frames[0];
    if metadata.header.frame_type != FrameType::RpcMetadata {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!(
                "first response frame must be RpcMetadata, got {:?}",
                metadata.header.frame_type
            ),
        ));
    }

    let completion = frames.last().ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Protocol,
            "response completion frame is missing",
        )
    })?;
    if completion.header.frame_type != FrameType::RpcCompletion {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Protocol,
            format!(
                "last response frame must be RpcCompletion, got {:?}",
                completion.header.frame_type
            ),
        ));
    }

    for (index, frame) in frames.iter().enumerate() {
        if frame.header.request_id != request_id || frame.header.session_id != session_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!("response frame {index} lost request/session correlation"),
            ));
        }

        if index > 0 && index + 1 < frames.len() && frame.header.frame_type != FrameType::RpcBatch {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!(
                    "intermediate response frame {index} must be RpcBatch, got {:?}",
                    frame.header.frame_type
                ),
            ));
        }
    }

    Ok(())
}

pub(crate) fn allocate_test_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}
