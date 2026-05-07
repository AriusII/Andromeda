import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "andromeda_policy_gate.py"
SPEC = importlib.util.spec_from_file_location("andromeda_policy_gate", SCRIPT)
andromeda_policy_gate = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = andromeda_policy_gate
SPEC.loader.exec_module(andromeda_policy_gate)


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def violation_codes(root: Path) -> set[str]:
    return {violation.code for violation in andromeda_policy_gate.run_policy_gate(root)}


class AndromedaPolicyGateTests(unittest.TestCase):
    def test_negative_doctrine_docs_and_skills_do_not_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "docs" / "doctrine.md",
                """
                # Doctrine

                gRPC, tonic, SELECT *, serde_json, sqlx, tokio-postgres,
                rusqlite, and SRPL while loops are named here only as
                forbidden technologies.
                """,
            )
            write(
                root / ".agents" / "skills" / "no-grpc-enforcement" / "SKILL.md",
                """
                Search for gRPC and tonic references, classify them, and block
                active drift only.
                """,
            )
            write(
                root / ".github" / "docs" / "BRANCH_PROTECTION.md",
                "The policy gate blocks ad hoc SQL and gRPC on runtime surfaces.",
            )

            self.assertEqual(violation_codes(root), set())

    def test_tests_comments_and_guardrails_do_not_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "crates" / "andromeda-quic" / "tests" / "doctrine_gate.rs",
                """
                #[test]
                fn forbids_forbidden_terms() {
                    let forbidden = ["grpc", "tonic", "serde_json", "SELECT *", "while"];
                    assert!(forbidden.contains(&"grpc"));
                }
                """,
            )
            write(
                root
                / "crates"
                / "andromeda-srpl"
                / "tests"
                / "fixtures"
                / "bad_loop.srpl",
                """
                procedure Inventory.BadLoop
                body {
                  while true do emit Nothing;
                }
                """,
            )
            write(
                root
                / "crates"
                / "andromeda-proto"
                / "proto"
                / "andromeda"
                / "protocol"
                / "v1"
                / "payload.proto",
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                // No JSON, gRPC, tonic, service, rpc, or SELECT * surface.
                message BinaryPayload {
                  bytes payload = 1; // never JSON
                  reserved 2 to 15;
                }
                """,
            )
            write(
                root / "crates" / "andromeda-proto" / "build.rs",
                """
                fn reject_forbidden_schema_text(text: &str) -> Option<&'static str> {
                    let lower = text.to_ascii_lowercase();
                    if lower.contains("grpc") { return Some("grpc"); }
                    if lower.contains("tonic") { return Some("tonic"); }
                    None
                }
                """,
            )
            write(
                root / "crates" / "andromeda-quic" / "src" / "comments_only.rs",
                """
                //! Guardrail text may mention tonic::transport and serde_json::Value.
                pub fn binary_payload(bytes: &[u8]) -> &[u8] {
                    // grpc::Client and SELECT * are forbidden examples only.
                    bytes
                }
                """,
            )
            write(
                root / "crates" / "andromeda-srpl" / "src" / "source_location.rs",
                """
                #[cfg(test)]
                mod tests {
                    #[test]
                    fn select_star_is_rejected_by_the_srpl_scanner() {
                        let source = "procedure X begin SELECT * from T end;";
                        assert!(source.contains("SELECT *"));
                    }
                }
                """,
            )

            self.assertEqual(violation_codes(root), set())

    def test_runtime_source_drift_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "crates" / "andromeda-quic" / "src" / "gateway.rs",
                """
                pub fn unsafe_runtime_api(bytes: &[u8]) {
                    let _client = grpc::Client::new();
                    let _channel = tonic::transport::Channel::from_static("https://localhost");
                    let _wire_format = PayloadEncoding::Json;
                    let _content_type = "application/json";
                    let _payload = serde_json::from_slice::<serde_json::Value>(bytes);
                    let _exec = execute_sql;
                    let _sql = "SELECT * FROM ProductStock";
                    let _insert = "INSERT INTO ProductStock VALUES (1)";
                    let _text = query_text;
                }
                """,
            )

            codes = violation_codes(root)

        self.assertIn("grpc_binding", codes)
        self.assertIn("tonic_binding", codes)
        self.assertIn("json_runtime_wire", codes)
        self.assertIn("json_runtime_default", codes)
        self.assertIn("sql_select_star", codes)
        self.assertIn("sql_dml_string", codes)
        self.assertIn("sql_text_surface", codes)

    def test_dependency_manifest_drift_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "crates" / "andromeda-quic" / "Cargo.toml",
                """
                [dependencies]
                prost-grpc = "0.4"
                tonic = "0.12"
                tonic-reflection = "0.12"
                serde_json = "1"
                """,
            )
            write(
                root / "crates" / "andromeda-exec" / "Cargo.toml",
                """
                [dependencies]
                sqlx = "0.8"
                """,
            )

            codes = violation_codes(root)

        self.assertIn("grpc_dependency", codes)
        self.assertIn("tonic_dependency", codes)
        self.assertIn("json_runtime_dependency", codes)
        self.assertIn("sql_dependency", codes)

    def test_protocol_schema_drift_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root
                / "crates"
                / "andromeda-proto"
                / "proto"
                / "andromeda"
                / "protocol"
                / "v1"
                / "command.proto",
                """
                syntax = "proto3";
                package andromeda.protocol.v1;
                service SqlGateway {
                  rpc Execute (Request) returns (Response);
                }
                message Request {
                  string query_text = 1;
                  string json_payload = 2;
                  string while_condition = 3;
                  reserved 4 to 15;
                }
                message Response { reserved 1 to 15; }
                """,
            )

            codes = violation_codes(root)

        self.assertIn("protobuf_service", codes)
        self.assertIn("protobuf_rpc", codes)
        self.assertIn("json_wire", codes)
        self.assertIn("sql_surface", codes)
        self.assertIn("srpl_unbounded_loop_surface", codes)

    def test_active_srpl_source_unbounded_loop_keyword_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "crates" / "andromeda-srpl" / "procedures" / "bad_loop.srpl",
                """
                procedure Inventory.BadLoop
                body {
                  repeat until Reserved;
                }
                """,
            )

            codes = violation_codes(root)

        self.assertIn("srpl_unbounded_loop_keyword", codes)

    def test_gpu_reference_on_critical_path_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root
                / "crates"
                / "andromeda-storage"
                / "src"
                / "write_ahead_log"
                / "append.rs",
                """
                pub fn append_commit_record() {
                    let _gpu = GpuProfile::disabled();
                }
                """,
            )
            write(
                root / "crates" / "andromeda-tx" / "src" / "mvcc.rs",
                """
                pub fn visible_at_snapshot() {
                    let _kernel = cuda::Kernel;
                }
                """,
            )

            codes = violation_codes(root)

        self.assertIn("gpu_critical_path", codes)

    def test_gpu_policy_modules_and_comments_do_not_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root
                / "crates"
                / "andromeda-storage"
                / "src"
                / "operational_profile"
                / "profile.rs",
                """
                pub fn validate_gpu_pipeline(profile: OperationalProfile) -> bool {
                    profile.hardware.gpu.available
                }
                """,
            )
            write(
                root
                / "crates"
                / "andromeda-storage"
                / "src"
                / "write_ahead_log"
                / "append.rs",
                """
                //! GPU is forbidden here by doctrine.
                pub fn append_commit_record(bytes: &[u8]) -> &[u8] {
                    bytes
                }
                """,
            )

            self.assertEqual(violation_codes(root), set())


if __name__ == "__main__":
    unittest.main()
