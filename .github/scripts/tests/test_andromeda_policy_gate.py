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

    def test_dependency_manifest_alias_and_workspace_alias_drift_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "Cargo.toml",
                """
                [workspace]
                members = ["crates/andromeda-runtime"]

                [workspace.dependencies]
                sql-driver = { package = "sqlx", version = "0.8" }
                grpc-stack = { package = "tonic", version = "0.12" }
                json-wire = { package = "serde_json", version = "1" }
                """,
            )
            write(
                root / "crates" / "andromeda-runtime" / "Cargo.toml",
                """
                [package]
                name = "andromeda-runtime"
                version = "0.0.0"
                edition = "2024"

                [dependencies]
                local-sql = { package = "tokio-postgres", version = "0.7" }
                local-grpc = { package = "prost-grpc", version = "0.4" }
                local-json = { package = "jsonrpsee", version = "0.24" }
                sql-driver = { workspace = true }
                grpc-stack = { workspace = true }
                json-wire = { workspace = true }
                """,
            )

            violations = andromeda_policy_gate.run_policy_gate(root)
            codes = {violation.code for violation in violations}
            excerpts = "\n".join(violation.excerpt for violation in violations)

        self.assertIn("sql_dependency", codes)
        self.assertIn("grpc_dependency", codes)
        self.assertIn("tonic_dependency", codes)
        self.assertIn("json_runtime_dependency", codes)
        self.assertIn("workspace=True", excerpts)
        self.assertIn("local-sql package=tokio-postgres", excerpts)

    def test_exec_runtime_quinn_reexport_boundary_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "crates" / "andromeda-exec" / "Cargo.toml",
                """
                [package]
                name = "andromeda-exec"
                version = "0.0.0"
                edition = "2024"

                [dependencies]
                andromeda-quic = "0.0.0"

                [features]
                default = []
                runtime-quinn = ["andromeda-quic/runtime-quinn"]
                """,
            )
            write(
                root / "crates" / "andromeda-exec" / "src" / "gateway.rs",
                """
                pub fn routes_through_abstract_transport() {
                    let _surface = "andromeda_quic abstract transport";
                }
                """,
            )

            self.assertEqual(violation_codes(root), set())

    def test_exec_runtime_quinn_feature_guard_fails_direct_runtime_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root / "crates" / "andromeda-exec" / "Cargo.toml",
                """
                [package]
                name = "andromeda-exec"
                version = "0.0.0"
                edition = "2024"

                [dependencies]
                andromeda-quic = { version = "0.0.0", features = ["runtime-quinn"] }
                quinn = "0.11"
                rcgen = "0.14"
                rustls = "0.23"
                tonic = "0.12"
                grpcio = "0.13"
                serde_json = "1"

                [features]
                default = ["runtime-quinn"]
                runtime-quinn = ["andromeda-quic/runtime-quinn"]
                """,
            )
            write(
                root / "crates" / "andromeda-exec" / "src" / "gateway.rs",
                """
                pub fn opens_concrete_runtime() {
                    let _conn = quinn::Connection;
                    let _tls = rustls::ClientConfig;
                    let _cert = rcgen::CertificateParams;
                    let _channel = tonic::transport::Channel;
                    let _payload = serde_json::Value::Null;
                }
                """,
            )

            codes = violation_codes(root)

        self.assertIn("exec_concrete_runtime_dependency", codes)
        self.assertIn("exec_runtime_quinn_dependency_feature", codes)
        self.assertIn("exec_runtime_quinn_default", codes)
        self.assertIn("exec_concrete_runtime_binding", codes)

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
            write(
                root
                / "crates"
                / "andromeda-observe"
                / "src"
                / "events"
                / "durable_audit"
                / "append.rs",
                """
                pub fn append_security_audit_record() {
                    let _profile = GpuProfile::disabled();
                }
                """,
            )

            codes = violation_codes(root)

        self.assertIn("gpu_critical_path", codes)

    def test_gpu_reference_on_storage_placement_manifest_page_heap_buffer_and_disk_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in (
                ("placement", "policy.rs"),
                ("manifest", "publish.rs"),
                ("page", "format.rs"),
                ("heap", "record.rs"),
                ("buffer_pool", "evict.rs"),
                ("disk_manager", "write.rs"),
            ):
                write(
                    root / "crates" / "andromeda-storage" / "src" / relative[0] / relative[1],
                    """
                    pub fn critical_storage_operation() {
                        let _kernel = cuda::Kernel;
                    }
                    """,
                )
            write(
                root / "crates" / "andromeda-storage" / "src" / "storage_placement.rs",
                """
                pub fn select_durable_location() {
                    let _profile = GpuProfile::disabled();
                }
                """,
            )
            write(
                root / "crates" / "andromeda-storage" / "src" / "disk_manager.rs",
                """
                pub fn flush_page() {
                    let _profile = wgpu::Device;
                }
                """,
            )

            violations = andromeda_policy_gate.run_policy_gate(root)
            gpu_violations = [
                violation for violation in violations if violation.code == "gpu_critical_path"
            ]

        self.assertEqual(len(gpu_violations), 8)

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
            write(
                root
                / "crates"
                / "andromeda-observe"
                / "src"
                / "query"
                / "event_family.rs",
                """
                pub enum TraceEventFamily {
                    Gpu,
                }

                pub fn classify_gpu_policy_event() -> TraceEventFamily {
                    TraceEventFamily::Gpu
                }
                """,
            )

            self.assertEqual(violation_codes(root), set())

    def test_gpu_boundary_denial_evidence_on_critical_paths_does_not_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(
                root
                / "crates"
                / "andromeda-storage"
                / "src"
                / "manifest"
                / "cold_publication.rs",
                """
                pub fn validate(decision: Decision) -> Result<(), Error> {
                    if decision.gpu_enabled {
                        return Err(Error::new("ColdStore segment publication plan must not enable GPU execution"));
                    }
                    Ok(())
                }
                """,
            )
            write(
                root
                / "crates"
                / "andromeda-storage"
                / "src"
                / "placement"
                / "policy.rs",
                """
                pub struct Request {
                    pub use_gpu: bool,
                }

                pub struct Decision {
                    pub gpu_enabled: bool,
                }

                pub fn plan(request: Request, hardware: HardwareProfile) -> Result<Decision, Error> {
                    if request.use_gpu {
                        hardware.validate_gpu_pipeline(PipelineClass::BackgroundMaintenance)?;
                    }
                    Ok(Decision { gpu_enabled: request.use_gpu })
                }
                """,
            )
            write(
                root
                / "crates"
                / "andromeda-observe"
                / "src"
                / "events"
                / "durable_audit"
                / "event_mapping.rs",
                """
                pub fn durable_audit_family(event: TraceEvent) -> Option<Family> {
                    match event {
                        TraceEvent::GpuPolicyDecision(_) => None,
                    }
                }
                """,
            )

            self.assertEqual(violation_codes(root), set())


if __name__ == "__main__":
    unittest.main()
