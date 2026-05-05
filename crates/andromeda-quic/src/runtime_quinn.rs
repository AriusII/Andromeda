//! Opt-in Quinn runtime dependency wiring.
//!
//! This module is intentionally private and feature-gated. It proves that the
//! `runtime-quinn` feature resolves the selected QUIC/TLS/runtime crates without
//! exposing `quinn`, `rustls`, `rcgen`, or executor types through the public
//! transport trait boundary.
//!
//! H2-QUIC-003 adds TLS configuration construction only. Real sockets,
//! listeners, stream management, and handshake execution remain deferred.

use std::{path::Path, sync::Arc};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{CertificateIdentity, SurfaceScope};
use rustls::{
    RootCertStore,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject},
};

use crate::{
    EarlyDataPolicy,
    mtls_identity::{ParsedCertificate, RawCertificate},
};

/// Returns `true` when the `runtime-quinn` feature is active and its dependency
/// crates are available to this crate.
///
/// The type-name references are compile-time wiring checks only; no networking,
/// executor startup, or I/O occurs.
#[allow(dead_code)]
pub(crate) fn runtime_quinn_dependencies_available() -> bool {
    let _ = core::any::type_name::<quinn::Connection>();
    let _ = core::any::type_name::<quinn::crypto::rustls::QuicClientConfig>();
    let _ = core::any::type_name::<quinn::crypto::rustls::QuicServerConfig>();
    let _ = core::any::type_name::<rcgen::CertificateParams>();
    let _ = core::any::type_name::<rustls::ClientConfig>();
    let _ = core::any::type_name::<tokio::runtime::Handle>();

    true
}

/// Replay class used by the TLS runtime adapter when evaluating 0-RTT policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestReplayClass {
    /// Request may mutate durable or externally visible state.
    Mutating,
    /// Request has been proven replay-safe by a future higher-level contract.
    ReplaySafe,
}

/// TLS early-data policy applied to rustls configs before any Quinn wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TlsEarlyDataPolicy {
    early_data: EarlyDataPolicy,
}

impl TlsEarlyDataPolicy {
    /// Canonical Andromeda policy: TLS 0-RTT is disabled for every request
    /// class until a later doctrine decision scopes replay-safe semantics.
    pub(crate) const fn disabled() -> Self {
        Self {
            early_data: EarlyDataPolicy::Disabled,
        }
    }

    /// Returns the runtime-free listener policy this TLS policy enforces.
    pub(crate) const fn early_data_policy(&self) -> EarlyDataPolicy {
        self.early_data
    }

    /// Returns whether TLS 0-RTT may be used for a request replay class.
    pub(crate) const fn allows_early_data_for(&self, _class: RequestReplayClass) -> bool {
        match self.early_data {
            EarlyDataPolicy::Disabled => false,
        }
    }

    fn apply_to_server_config(&self, config: &mut rustls::ServerConfig) {
        match self.early_data {
            EarlyDataPolicy::Disabled => {
                config.max_early_data_size = 0;
                config.send_half_rtt_data = false;
            }
        }
    }

    fn apply_to_client_config(&self, config: &mut rustls::ClientConfig) {
        match self.early_data {
            EarlyDataPolicy::Disabled => {
                config.enable_early_data = false;
            }
        }
    }
}

impl Default for TlsEarlyDataPolicy {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Private rustls config bundle used by future Quinn listener/client adapters.
///
/// The concrete rustls types are intentionally contained in this private,
/// feature-gated module; the default public transport API remains runtime-free.
#[derive(Clone)]
pub(crate) struct RuntimeQuinnTlsConfig {
    pub(crate) server: Arc<rustls::ServerConfig>,
    pub(crate) client: Arc<rustls::ClientConfig>,
    pub(crate) identity_extractor: CertificateIdentityExtraction,
    early_data_policy: TlsEarlyDataPolicy,
}

impl core::fmt::Debug for RuntimeQuinnTlsConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeQuinnTlsConfig")
            .field("server", &"<rustls::ServerConfig>")
            .field("client", &"<rustls::ClientConfig>")
            .field("identity_extractor", &self.identity_extractor)
            .field("early_data_policy", &self.early_data_policy)
            .finish()
    }
}

impl RuntimeQuinnTlsConfig {
    /// Returns the 0-RTT policy encoded into this bundle.
    pub(crate) const fn early_data_policy(&self) -> TlsEarlyDataPolicy {
        self.early_data_policy
    }

    /// Returns true when mutating requests are barred from TLS 0-RTT.
    pub(crate) const fn disables_zero_rtt_for_mutating_requests(&self) -> bool {
        !self
            .early_data_policy
            .allows_early_data_for(RequestReplayClass::Mutating)
    }
}

/// Identity extraction hook carried beside the TLS configs.
///
/// The hook converts a peer certificate chain as exposed by rustls/quinn into
/// the existing runtime-free identity contracts. It does not perform a
/// handshake and it deliberately does not parse X.509 in this module; callers
/// can inject a parser that returns [`ParsedCertificate`].
#[derive(Clone)]
pub(crate) struct CertificateIdentityExtraction {
    required_scope: SurfaceScope,
    parser: Option<Arc<ParsedCertificateHook>>,
}

type ParsedCertificateHook =
    dyn Fn(RawCertificate) -> AndromedaResult<ParsedCertificate> + Send + Sync + 'static;

impl core::fmt::Debug for CertificateIdentityExtraction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CertificateIdentityExtraction")
            .field("required_scope", &self.required_scope)
            .field("parser_installed", &self.parser.is_some())
            .finish()
    }
}

impl CertificateIdentityExtraction {
    /// Creates an extraction hook for a required surface scope.
    pub(crate) const fn deferred(required_scope: SurfaceScope) -> Self {
        Self {
            required_scope,
            parser: None,
        }
    }

    /// Creates an extraction hook with an injected DER parser.
    pub(crate) fn with_parser(
        required_scope: SurfaceScope,
        parser: impl Fn(RawCertificate) -> AndromedaResult<ParsedCertificate> + Send + Sync + 'static,
    ) -> Self {
        Self {
            required_scope,
            parser: Some(Arc::new(parser)),
        }
    }

    /// Surface scope that extracted identities must be bound to.
    pub(crate) const fn required_scope(&self) -> SurfaceScope {
        self.required_scope
    }

    /// Converts the first peer certificate into [`RawCertificate`].
    pub(crate) fn first_raw_certificate(
        &self,
        peer_chain: &[CertificateDer<'_>],
    ) -> Option<RawCertificate> {
        peer_chain
            .first()
            .map(|cert| RawCertificate::new(cert.as_ref().to_vec()))
    }

    /// Extracts a [`CertificateIdentity`] when a parser hook has been installed.
    ///
    /// Returning `Ok(None)` means no certificate was available. Returning a
    /// security error for a present certificate with no parser keeps X.509
    /// parsing explicitly deferred rather than using ad hoc parsing.
    pub(crate) fn extract_identity(
        &self,
        peer_chain: &[CertificateDer<'_>],
    ) -> AndromedaResult<Option<CertificateIdentity>> {
        let Some(raw) = self.first_raw_certificate(peer_chain) else {
            return Ok(None);
        };

        let Some(parser) = &self.parser else {
            return Err(security_error(
                "certificate identity parser hook not installed",
            ));
        };

        parser(raw)?
            .to_certificate_identity(self.required_scope)
            .map(Some)
    }
}

/// Builds an ephemeral mTLS-capable rustls config pair for tests.
///
/// A self-signed rcgen certificate is trusted by both sides and installed as
/// both the server certificate and the client-auth identity certificate. This
/// function performs no network I/O and starts no executor.
#[allow(dead_code)]
pub(crate) fn ephemeral_test_tls_config(
    required_scope: SurfaceScope,
) -> AndromedaResult<RuntimeQuinnTlsConfig> {
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(["localhost".to_string()]).map_err(|err| {
            security_error(format!(
                "failed to generate ephemeral test certificate: {err}"
            ))
        })?;

    let cert_chain = vec![cert.der().clone()];
    let key_der = key_pair.serialize_der();
    let trust_roots = cert_chain.clone();

    build_mtls_configs_from_der(
        cert_chain.clone(),
        private_key_from_pkcs8_der(key_der.clone()),
        cert_chain,
        private_key_from_pkcs8_der(key_der),
        trust_roots,
        CertificateIdentityExtraction::deferred(required_scope),
        TlsEarlyDataPolicy::disabled(),
    )
}

/// Builds rustls server/client configs from file-backed PEM certificates.
///
/// No additional PEM parser dependency is introduced: this uses the PEM support
/// re-exported by `rustls::pki_types`. The same identity cert/key is installed
/// on the server and client sides as a narrow H2-QUIC-003 scaffold; real socket
/// backends and separate deployment identities are deferred.
#[allow(dead_code)]
pub(crate) fn file_backed_mtls_config_from_pem_files(
    identity_cert_chain_pem: impl AsRef<Path>,
    identity_private_key_pem: impl AsRef<Path>,
    trust_roots_pem: impl AsRef<Path>,
    required_scope: SurfaceScope,
) -> AndromedaResult<RuntimeQuinnTlsConfig> {
    let identity_cert_chain = load_cert_chain_pem(identity_cert_chain_pem.as_ref())?;
    let trust_roots = load_cert_chain_pem(trust_roots_pem.as_ref())?;

    build_mtls_configs_from_der(
        identity_cert_chain.clone(),
        load_private_key_pem(identity_private_key_pem.as_ref())?,
        identity_cert_chain,
        load_private_key_pem(identity_private_key_pem.as_ref())?,
        trust_roots,
        CertificateIdentityExtraction::deferred(required_scope),
        TlsEarlyDataPolicy::disabled(),
    )
}

/// Builds rustls server/client configs from file-backed DER certificates.
///
/// The private key is expected to be unencrypted PKCS#8 DER. Supporting other
/// key formats for DER files should be added deliberately with tests.
#[allow(dead_code)]
pub(crate) fn file_backed_mtls_config_from_der_files(
    identity_cert_der: impl AsRef<Path>,
    identity_private_key_pkcs8_der: impl AsRef<Path>,
    trust_root_der: impl AsRef<Path>,
    required_scope: SurfaceScope,
) -> AndromedaResult<RuntimeQuinnTlsConfig> {
    let identity_cert_chain = vec![load_certificate_der(identity_cert_der.as_ref())?];
    let key_bytes = std::fs::read(identity_private_key_pkcs8_der.as_ref()).map_err(|err| {
        security_error(format!(
            "failed to read DER private key {}: {err}",
            identity_private_key_pkcs8_der.as_ref().display()
        ))
    })?;
    let trust_roots = vec![load_certificate_der(trust_root_der.as_ref())?];

    build_mtls_configs_from_der(
        identity_cert_chain.clone(),
        private_key_from_pkcs8_der(key_bytes.clone()),
        identity_cert_chain,
        private_key_from_pkcs8_der(key_bytes),
        trust_roots,
        CertificateIdentityExtraction::deferred(required_scope),
        TlsEarlyDataPolicy::disabled(),
    )
}

fn build_mtls_configs_from_der(
    server_cert_chain: Vec<CertificateDer<'static>>,
    server_private_key: PrivateKeyDer<'static>,
    client_cert_chain: Vec<CertificateDer<'static>>,
    client_private_key: PrivateKeyDer<'static>,
    trust_roots: Vec<CertificateDer<'static>>,
    identity_extractor: CertificateIdentityExtraction,
    early_data_policy: TlsEarlyDataPolicy,
) -> AndromedaResult<RuntimeQuinnTlsConfig> {
    if server_cert_chain.is_empty() {
        return Err(security_error("server certificate chain cannot be empty"));
    }
    if client_cert_chain.is_empty() {
        return Err(security_error("client certificate chain cannot be empty"));
    }

    let root_store = root_store_from_der(trust_roots)?;
    let client_verifier =
        rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store.clone()))
            .build()
            .map_err(|err| {
                security_error(format!(
                    "failed to build client certificate verifier: {err}"
                ))
            })?;

    let mut server = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(server_cert_chain, server_private_key)
        .map_err(|err| security_error(format!("failed to build server TLS config: {err}")))?;
    early_data_policy.apply_to_server_config(&mut server);

    let mut client = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(client_cert_chain, client_private_key)
        .map_err(|err| security_error(format!("failed to build client TLS config: {err}")))?;
    early_data_policy.apply_to_client_config(&mut client);

    Ok(RuntimeQuinnTlsConfig {
        server: Arc::new(server),
        client: Arc::new(client),
        identity_extractor,
        early_data_policy,
    })
}

fn root_store_from_der(
    trust_roots: Vec<CertificateDer<'static>>,
) -> AndromedaResult<RootCertStore> {
    if trust_roots.is_empty() {
        return Err(security_error("trust root set cannot be empty"));
    }

    let mut roots = RootCertStore::empty();
    for root in trust_roots {
        roots
            .add(root)
            .map_err(|err| security_error(format!("failed to add trust root: {err}")))?;
    }
    Ok(roots)
}

fn load_cert_chain_pem(path: &Path) -> AndromedaResult<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_file_iter(path)
        .map_err(|err| {
            security_error(format!(
                "failed to open PEM certificate file {}: {err}",
                path.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            security_error(format!(
                "failed to parse PEM certificate file {}: {err}",
                path.display()
            ))
        })?;

    if certs.is_empty() {
        return Err(security_error(format!(
            "PEM certificate file {} did not contain certificates",
            path.display()
        )));
    }
    Ok(certs)
}

fn load_private_key_pem(path: &Path) -> AndromedaResult<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path).map_err(|err| {
        security_error(format!(
            "failed to parse PEM private key {}: {err}",
            path.display()
        ))
    })
}

fn load_certificate_der(path: &Path) -> AndromedaResult<CertificateDer<'static>> {
    std::fs::read(path)
        .map(CertificateDer::from)
        .map_err(|err| {
            security_error(format!(
                "failed to read DER certificate {}: {err}",
                path.display()
            ))
        })
}

fn private_key_from_pkcs8_der(key_der: Vec<u8>) -> PrivateKeyDer<'static> {
    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der))
}

fn security_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_quinn_dependency_wiring_is_available() {
        assert!(runtime_quinn_dependencies_available());
    }

    #[test]
    fn ephemeral_test_config_disables_zero_rtt() {
        let bundle = ephemeral_test_tls_config(SurfaceScope::Application).unwrap();

        assert_eq!(
            bundle.early_data_policy().early_data_policy(),
            EarlyDataPolicy::Disabled
        );
        assert!(bundle.disables_zero_rtt_for_mutating_requests());
        assert!(
            !bundle
                .early_data_policy()
                .allows_early_data_for(RequestReplayClass::Mutating)
        );
        assert!(
            !bundle
                .early_data_policy()
                .allows_early_data_for(RequestReplayClass::ReplaySafe)
        );
        assert_eq!(bundle.server.max_early_data_size, 0);
        assert!(!bundle.server.send_half_rtt_data);
        assert!(!bundle.client.enable_early_data);
    }

    #[test]
    fn identity_hook_extracts_first_raw_certificate() {
        let hook = CertificateIdentityExtraction::deferred(SurfaceScope::Cluster);
        let cert = CertificateDer::from(vec![0x30, 0x82, 0x01, 0x02]);

        let raw = hook.first_raw_certificate(&[cert]).unwrap();

        assert_eq!(hook.required_scope(), SurfaceScope::Cluster);
        assert_eq!(raw.der_bytes, vec![0x30, 0x82, 0x01, 0x02]);
    }

    #[test]
    fn identity_hook_without_parser_defers_x509_parsing() {
        let hook = CertificateIdentityExtraction::deferred(SurfaceScope::Application);
        let cert = CertificateDer::from(vec![0x30, 0x82, 0x01, 0x02]);

        let error = hook.extract_identity(&[cert]).unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Security);
        assert_eq!(
            error.message(),
            "certificate identity parser hook not installed"
        );
    }

    #[test]
    fn identity_hook_with_parser_builds_certificate_identity() {
        let hook =
            CertificateIdentityExtraction::with_parser(SurfaceScope::MonitoringAgent, |raw| {
                assert_eq!(raw.der_bytes, vec![0x30, 0x82]);
                ParsedCertificate::new("monitor".to_string(), "a".repeat(64), None)
            });
        let cert = CertificateDer::from(vec![0x30, 0x82]);

        let identity = hook.extract_identity(&[cert]).unwrap().unwrap();

        assert_eq!(identity.subject, "monitor");
        assert_eq!(identity.fingerprint, "a".repeat(64));
        assert_eq!(identity.surface, SurfaceScope::MonitoringAgent);
    }
}
