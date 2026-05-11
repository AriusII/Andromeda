use andromeda_error::AndromedaErrorKind;
use andromeda_principal::{CertificateIdentity, SurfaceScope};
use andromeda_procedure_contract::{
    ProcedureGatewayManifest as CatalogProcedureManifest,
    ProcedureGatewayProtocolLayout as CatalogProcedureProtocolLayout,
    ProcedureGatewayRequiredPermission as CatalogRequiredPermission,
};
use andromeda_proto::generated::{self, encode_generated_message};
use andromeda_quic::{Connection, LifecycleState, ProcedureGateway, SurfacePlane};
use andromeda_rpc_protocol::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameHeader, FrameType, PayloadKind,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId, RequestId, SessionId};

const ROUTE_SOURCE: &str = include_str!("../src/procedure_gateway/route.rs");
const GATEWAY_SOURCE: &str = include_str!("../src/procedure_gateway.rs");
const SURFACE_SOURCE: &str = include_str!("../src/connection/surface.rs");
const HADR_STREAM_MIN: u64 = 128;

fn compact(source: &str) -> String {
    source.split_whitespace().collect::<String>()
}

fn hash(byte: u8) -> ContractHash {
    ContractHash::test_vector(byte)
}

fn hello_frame(session_id: u64) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::Hello,
            request_id: RequestId::new(1),
            session_id: SessionId::new(session_id),
            tx_id: None,
            payload_length: 0,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: Vec::new(),
    }
}

fn auth_frame(session_id: u64) -> FrameBytes {
    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::Auth,
            request_id: RequestId::new(1),
            session_id: SessionId::new(session_id),
            tx_id: None,
            payload_length: 0,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload: Vec::new(),
    }
}

fn setup_active_application_connection() -> Connection {
    let mut conn = Connection::new(SurfacePlane::Application);
    let identity = CertificateIdentity::new(
        "a".repeat(64),
        "app-service".to_string(),
        SurfaceScope::Application,
    )
    .unwrap();
    conn.set_certificate_identity(identity).unwrap();
    conn.accept_hello(&hello_frame(100)).unwrap();
    conn.accept_auth(&auth_frame(100)).unwrap();
    assert_eq!(conn.state(), LifecycleState::Active);
    conn
}

fn route_manifest() -> CatalogProcedureManifest {
    CatalogProcedureManifest {
        procedure_id: ProcedureId::new(42),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: hash(0x11),
        catalog_version: CatalogVersion::new(9),
        protocol_layout: CatalogProcedureProtocolLayout {
            descriptor_set_hash: hash(0x22),
            frame_envelope_hash: hash(0x33),
            protocol_package: "andromeda.protocol.v1".to_string(),
            contract_package: "andromeda.contract.v1".to_string(),
        },
        result_streams: Vec::new(),
        stats_version: 5,
        policy_version: hash(0x44),
        required_permissions: vec![CatalogRequiredPermission {
            id: "andromeda.execute_procedure".to_string(),
            family: "application".to_string(),
        }],
    }
}

fn valid_execute_frame(manifest: &CatalogProcedureManifest) -> FrameBytes {
    let execute_request = generated::protocol::v1::RpcExecuteRequest {
        procedure_name: manifest.procedure_name.clone(),
        expected_contract_hash: manifest.contract_hash.as_bytes().to_vec(),
        expected_catalog_version: manifest.catalog_version.get(),
        surface_scope: "application".to_string(),
        arguments: Vec::new(),
        budget: None,
        expected_stats_version: Some(manifest.stats_version),
    };
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated::protocol::v1::ProtocolVersion { major: 1, minor: 0 }),
        contract_hash: manifest.contract_hash.as_bytes().to_vec(),
        catalog_version: manifest.catalog_version.get(),
        request_id: 501,
        session_id: 100,
        tx_id: None,
        payload_kind: PayloadKind::RpcExecuteRequest.wire_code() as i32,
        payload: encode_generated_message(&execute_request),
    };
    let payload = encode_generated_message(&envelope);

    FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id: RequestId::new(501),
            session_id: SessionId::new(100),
            tx_id: None,
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    }
}

#[test]
fn application_route_rejects_hadr_reserved_stream_ids() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);

    let err = gateway
        .bind_application_procedure_route(HADR_STREAM_MIN, &frame, &manifest)
        .expect_err("HA/DR reserved stream range must stay outside application Procedure routing");

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("HA/DR reserved stream id"));
    assert!(err.message().contains("[128..=255]"));
}

#[test]
fn source_contract_keeps_hadr_namespace_explicit_and_outside_application_dispatch() {
    let compact_route = compact(ROUTE_SOURCE);
    assert!(compact_route.contains("constHADR_STREAM_MIN:u64=128;"));
    assert!(compact_route.contains("constHADR_STREAM_MAX:u64=255;"));
    assert!(compact_route.contains("(HADR_STREAM_MIN..=HADR_STREAM_MAX).contains(&stream_id)"));
    assert!(compact_route.contains("Applicationdispatch"));
    assert!(compact_route.contains("mustnotenter\\"));
}

#[test]
fn procedure_gateway_surface_contract_keeps_hadr_as_non_application_work() {
    let compact_gateway = compact(GATEWAY_SOURCE);
    let compact_surface = compact(SURFACE_SOURCE);

    assert!(compact_surface.contains("HighAvailability"));
    assert!(compact_surface.contains("HADR/clustercontrolplane"));
    assert!(compact_gateway.contains("bind_application_procedure_route"));
    assert!(
        compact_gateway.contains("Application-surfaceonly")
            || compact_gateway.contains("Application-surfaceonly;administration,HA/DR")
    );
    assert!(
        !compact_gateway.contains("bind_hadr")
            && !compact_gateway.contains("bind_cluster_promote")
            && !compact_gateway.contains("bind_cluster_fence"),
        "ProcedureGateway must not absorb HA/DR control-plane route ownership"
    );
}
