import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "protocol_doctrine_scan.py"
SPEC = importlib.util.spec_from_file_location("protocol_doctrine_scan", SCRIPT)
protocol_doctrine_scan = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = protocol_doctrine_scan
SPEC.loader.exec_module(protocol_doctrine_scan)

PROTO_MANIFEST_SCRIPT = Path(__file__).resolve().parents[1] / "proto_manifest.py"
PROTO_MANIFEST_SPEC = importlib.util.spec_from_file_location(
    "proto_manifest",
    PROTO_MANIFEST_SCRIPT,
)
proto_manifest = importlib.util.module_from_spec(PROTO_MANIFEST_SPEC)
assert PROTO_MANIFEST_SPEC.loader is not None
sys.modules[PROTO_MANIFEST_SPEC.name] = proto_manifest
PROTO_MANIFEST_SPEC.loader.exec_module(proto_manifest)


class ProtocolDoctrineScanTests(unittest.TestCase):
    def test_protobuf_manifest_includes_crate_local_protos(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            crate_proto = (
                root
                / "crates"
                / "andromeda-proto"
                / "proto"
                / "andromeda"
                / "protocol"
                / "v1"
                / "payload.proto"
            )
            root_proto = (
                root / "proto" / "andromeda" / "contract" / "v1" / "root.proto"
            )
            crate_proto.parent.mkdir(parents=True)
            root_proto.parent.mkdir(parents=True)
            crate_proto.write_text(
                'syntax = "proto3"; message CratePayload { bytes body = 1; }\n',
                encoding="utf-8",
            )
            root_proto.write_text(
                'syntax = "proto3"; message RootPayload { bytes body = 1; }\n',
                encoding="utf-8",
            )

            manifest = proto_manifest.build_manifest(root)

        paths = [contract["path"] for contract in manifest["contracts"]]
        self.assertEqual(paths, sorted(paths))
        self.assertIn(
            "crates/andromeda-proto/proto/andromeda/protocol/v1/payload.proto",
            paths,
        )
        self.assertIn("proto/andromeda/contract/v1/root.proto", paths)
        self.assertEqual(manifest["contract_count"], len(paths))

    def test_proto_schema_scan_rejects_service_declaration(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "service.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                service Catalog {
                  rpc Resolve (Request) returns (Response);
                }
                message Request { reserved 1 to 15; }
                message Response { reserved 1 to 15; }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), ())

        self.assertEqual(report["status"], "FAIL")
        codes = {violation["code"] for violation in report["violations"]}
        self.assertIn("protobuf_service", codes)
        self.assertIn("protobuf_rpc", codes)

    def test_proto_schema_scan_rejects_json_wire_usage(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "json_wire.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                message RuntimePayload {
                  string json_payload = 1;
                  reserved 2 to 15;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), ())

        self.assertEqual(report["status"], "FAIL")
        self.assertIn(
            "json_wire",
            {violation["code"] for violation in report["violations"]},
        )

    def test_proto_schema_scan_rejects_sql_query_surface_terms(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "catalog.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.contract.v1;
                message CatalogView {
                  uint64 view_id = 1;
                  bytes query_definition = 2;
                  string ad_hoc_sql = 3;
                  reserved 4 to 15;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), ())

        self.assertEqual(report["status"], "FAIL")
        self.assertIn(
            "sql_query_surface",
            {violation["code"] for violation in report["violations"]},
        )

    def test_proto_schema_scan_accepts_native_map_terms(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "catalog.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.contract.v1;
                message CatalogImmediateMap {
                  uint64 map_id = 1;
                  bytes map_definition = 2;
                  repeated uint64 depends_on_map_ids = 3;
                  reserved 4 to 15;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), ())

        self.assertEqual(report["status"], "PASS", report["violations"])

    def test_proto_schema_scan_ignores_doctrine_terms_in_comments(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "comments.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                // No JSON, gRPC, tonic, service, or rpc surface is allowed.
                message BinaryPayload {
                  bytes payload = 1; // never JSON
                  reserved 2 to 15;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), ())

        self.assertEqual(report["status"], "PASS", report["violations"])

    def test_proto_schema_scan_rejects_frame_code_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = (
                Path(directory)
                / "andromeda"
                / "protocol"
                / "v1"
                / "envelope.proto"
            )
            proto.parent.mkdir(parents=True)
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                enum PayloadKind {
                  PAYLOAD_KIND_UNSPECIFIED = 0;
                  PAYLOAD_KIND_HELLO = 2;
                  PAYLOAD_KIND_AUTH = 3;
                  PAYLOAD_KIND_CONTRACT_REQUEST = 4;
                  PAYLOAD_KIND_CONTRACT_RESPONSE = 5;
                  PAYLOAD_KIND_RPC_EXECUTE_REQUEST = 6;
                  PAYLOAD_KIND_RPC_METADATA = 7;
                  PAYLOAD_KIND_RPC_BATCH = 8;
                  PAYLOAD_KIND_RPC_COMPLETION = 9;
                  PAYLOAD_KIND_ERROR = 10;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((Path(directory),), ())

        self.assertEqual(report["status"], "FAIL")
        self.assertIn(
            "enum_drift",
            {violation["code"] for violation in report["violations"]},
        )

    def test_proto_schema_scan_rejects_google_protobuf_json_value_types(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "json_value.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                import "google/protobuf/struct.proto";
                message RuntimePayload {
                  google.protobuf.Struct active_json_object = 1;
                  reserved 2 to 15;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), ())

        self.assertEqual(report["status"], "FAIL")
        self.assertIn(
            "json_wire",
            {violation["code"] for violation in report["violations"]},
        )

    def test_scan_rejects_tonic_and_grpc_dependency_introduction(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = Path(directory) / "messages.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                message Payload { bytes body = 1; reserved 2 to 15; }
                """,
                encoding="utf-8",
            )
            manifest = Path(directory) / "Cargo.toml"
            manifest.write_text(
                """
                [dependencies]
                tonic = "0.12"
                grpcio = "0.13"
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), (manifest,))

        self.assertEqual(report["status"], "FAIL")
        codes = {violation["code"] for violation in report["violations"]}
        self.assertIn("tonic_reference", codes)
        self.assertIn("grpc_reference", codes)

    def test_scan_rejects_active_json_runtime_source_usage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            proto = root / "messages.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                message Payload { bytes body = 1; reserved 2 to 15; }
                """,
                encoding="utf-8",
            )
            source = root / "crates" / "andromeda-proto" / "src" / "lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                """
                pub fn parse_runtime_payload(bytes: &[u8]) {
                    let _ = serde_json::from_slice::<serde_json::Value>(bytes);
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), (source,))

        self.assertEqual(report["status"], "FAIL")
        self.assertIn(
            "json_wire",
            {violation["code"] for violation in report["violations"]},
        )

    def test_policy_scan_ignores_comments_and_documentation_examples(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            proto = root / "comments.proto"
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                // A forbidden example: service Example { rpc Run (A) returns (B); }
                message BinaryPayload {
                  bytes payload = 1;
                  reserved 2 to 15;
                }
                """,
                encoding="utf-8",
            )
            manifest = root / "Cargo.toml"
            manifest.write_text(
                """
                [dependencies]
                # tonic = "0.12"
                # grpcio = "0.13"
                """,
                encoding="utf-8",
            )
            source = root / "crates" / "andromeda-proto" / "src" / "lib.rs"
            source.parent.mkdir(parents=True)
            source.write_text(
                r'''
                //! Example text mentions tonic::transport and serde_json::Value.
                pub const DOC: &str = r#"grpc::Client and serde_json::Value are examples"#;
                pub fn binary_payload(bytes: &[u8]) -> &[u8] {
                    // tonic::transport::Channel is forbidden in active code.
                    bytes
                }
                ''',
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((proto,), (manifest, source))

        self.assertEqual(report["status"], "PASS", report["violations"])

    def test_status_enum_scan_rejects_unknown_acceptance_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            proto = (
                Path(directory)
                / "andromeda"
                / "protocol"
                / "v1"
                / "completion.proto"
            )
            proto.parent.mkdir(parents=True)
            proto.write_text(
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                message RpcCompletion {
                  enum Status {
                    STATUS_UNSPECIFIED = 0;
                    STATUS_COMMITTED = 1;
                    STATUS_ROLLED_BACK = 2;
                    STATUS_FAILED_BEFORE_TRANSACTION = 3;
                    STATUS_CANCELLED = 4;
                    STATUS_POISONED = 5;
                    STATUS_PERMISSION_DENIED = 6;
                    STATUS_CONTRACT_REJECTED = 7;
                    STATUS_SYSTEM_UNAVAILABLE = 8;
                    STATUS_ACCEPTED_UNKNOWN = 9;
                  }
                  Status status = 1;
                }
                """,
                encoding="utf-8",
            )

            report = protocol_doctrine_scan.run_scan((Path(directory),), ())

        self.assertEqual(report["status"], "FAIL")
        self.assertIn(
            "enum_drift",
            {violation["code"] for violation in report["violations"]},
        )


if __name__ == "__main__":
    unittest.main()
