//! TLS configuration for QUIC server and client.
//!
//! This module handles certificate generation (ephemeral or from disk) and
//! TLS config construction for both server and client sides.

use std::path::Path;
use std::sync::Arc;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
#[cfg(any(test, feature = "insecure-test-tls"))]
use rustls::pki_types::PrivatePkcs8KeyDer;
use rustls::{
    RootCertStore, ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};

/// TLS configuration builder for QUIC server.
pub struct ServerTlsConfig {
    config: ServerConfig,
}

impl ServerTlsConfig {
    /// Creates a self-trusting mTLS server config from certificate and private key files.
    ///
    /// The server certificate file is also used as the client-auth trust root.
    /// This test-only helper trusts the server certificate as the client-auth
    /// root. Production deployments must use [`Self::from_files_with_client_ca`]
    /// and pass an explicit client CA.
    #[cfg(any(test, feature = "insecure-test-tls"))]
    pub fn from_files(cert_path: &Path, key_path: &Path) -> AndromedaResult<Self> {
        Self::from_files_with_client_ca(cert_path, key_path, cert_path)
    }

    /// Creates an mTLS server config with an explicit PEM client CA trust root.
    pub fn from_files_with_client_ca(
        cert_path: &Path,
        key_path: &Path,
        client_ca_path: &Path,
    ) -> AndromedaResult<Self> {
        let certs = load_cert_chain_pem(cert_path)?;
        let key = load_private_key_pem(key_path)?;
        let client_roots = root_store_from_der(load_cert_chain_pem(client_ca_path)?)?;
        let client_verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(client_roots))
            .build()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Security,
                    format!("failed to build client certificate verifier: {e}"),
                )
            })?;

        let config = ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certs, key)
            .map_err(|e| tls_error(format!("failed to build server config: {e}")))?;
        let config = zero_rtt_disabled_server_config(config);

        Ok(Self { config })
    }

    /// Creates an ephemeral mTLS self-signed certificate for local testing.
    ///
    /// The generated certificate is trusted for client authentication. Tests
    /// that need a matching client identity should use the paired test config
    /// behind `insecure-test-tls`.
    #[cfg(any(test, feature = "insecure-test-tls"))]
    pub fn ephemeral(subject_alt_names: Vec<String>) -> AndromedaResult<Self> {
        Ok(Self {
            config: ephemeral_rustls_pair(subject_alt_names)?.server,
        })
    }

    /// Returns the configured Quinn server config.
    pub fn into_quinn_config(self) -> AndromedaResult<quinn::ServerConfig> {
        let quic_config =
            quinn::crypto::rustls::QuicServerConfig::try_from(self.config).map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Transport,
                    format!("failed to create Quinn server config: {e}"),
                )
            })?;

        Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_config)))
    }
}

/// TLS configuration builder for QUIC clients.
pub struct ClientTlsConfig;

impl ClientTlsConfig {
    /// Creates an mTLS client config from PEM identity and trust-root files.
    pub fn from_files(
        cert_path: &Path,
        key_path: &Path,
        trust_roots_path: &Path,
    ) -> AndromedaResult<quinn::ClientConfig> {
        let cfg = rustls::ClientConfig::builder()
            .with_root_certificates(root_store_from_der(load_cert_chain_pem(trust_roots_path)?)?)
            .with_client_auth_cert(
                load_cert_chain_pem(cert_path)?,
                load_private_key_pem(key_path)?,
            )
            .map_err(|e| tls_error(format!("failed to build client config: {e}")))?;
        let cfg = zero_rtt_disabled_client_config(cfg);

        quinn_client_config(cfg)
    }

    /// Creates a client TLS config that accepts all server certificates.
    ///
    /// This is intentionally gated behind `insecure-test-tls` and keeps client
    /// certificate authentication explicit even for local network tests.
    #[cfg(any(test, feature = "insecure-test-tls"))]
    pub fn insecure_for_tests(
        client_cert_chain: Vec<CertificateDer<'static>>,
        client_private_key: PrivateKeyDer<'static>,
    ) -> AndromedaResult<quinn::ClientConfig> {
        let cfg = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
            .with_client_auth_cert(client_cert_chain, client_private_key)
            .map_err(|e| tls_error(format!("failed to build insecure test client config: {e}")))?;
        let cfg = zero_rtt_disabled_client_config(cfg);

        quinn_client_config(cfg)
    }
}

/// Paired Quinn configs for local mTLS tests.
#[cfg(any(test, feature = "insecure-test-tls"))]
pub struct MutualTlsTestConfig {
    server: quinn::ServerConfig,
    client: quinn::ClientConfig,
}

#[cfg(any(test, feature = "insecure-test-tls"))]
impl MutualTlsTestConfig {
    /// Creates a self-contained mTLS config pair with one ephemeral identity.
    pub fn ephemeral(subject_alt_names: Vec<String>) -> AndromedaResult<Self> {
        let pair = ephemeral_rustls_pair(subject_alt_names)?;
        let server = ServerTlsConfig {
            config: pair.server,
        }
        .into_quinn_config()?;
        let client = quinn_client_config(pair.client)?;

        Ok(Self { server, client })
    }

    pub fn server_config(&self) -> quinn::ServerConfig {
        self.server.clone()
    }

    pub fn client_config(&self) -> quinn::ClientConfig {
        self.client.clone()
    }
}

#[cfg(any(test, feature = "insecure-test-tls"))]
struct RustlsMutualTlsPair {
    server: rustls::ServerConfig,
    client: rustls::ClientConfig,
}

#[cfg(any(test, feature = "insecure-test-tls"))]
fn ephemeral_rustls_pair(subject_alt_names: Vec<String>) -> AndromedaResult<RustlsMutualTlsPair> {
    use rcgen::generate_simple_self_signed;

    let rcgen::CertifiedKey { cert, signing_key } = generate_simple_self_signed(subject_alt_names)
        .map_err(|e| tls_error(format!("failed to generate self-signed certificate: {e}")))?;

    let cert_chain = vec![cert.der().clone()];
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
    let root_store = root_store_from_der(cert_chain.clone())?;
    let client_verifier =
        rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store.clone()))
            .build()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Security,
                    format!("failed to build client certificate verifier: {e}"),
                )
            })?;

    let server = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(cert_chain.clone(), private_key.clone_key())
        .map_err(|e| tls_error(format!("failed to build server config: {e}")))?;
    let server = zero_rtt_disabled_server_config(server);

    let client = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(cert_chain, private_key)
        .map_err(|e| tls_error(format!("failed to build client config: {e}")))?;
    let client = zero_rtt_disabled_client_config(client);

    Ok(RustlsMutualTlsPair { server, client })
}

fn load_cert_chain_pem(path: &Path) -> AndromedaResult<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_file_iter(path)
        .map_err(|e| tls_error(format!("failed to open certificate PEM: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| tls_error(format!("failed to parse certificate PEM: {e}")))?;

    if certs.is_empty() {
        return Err(tls_error("certificate PEM did not contain certificates"));
    }

    Ok(certs)
}

fn load_private_key_pem(path: &Path) -> AndromedaResult<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path)
        .map_err(|e| tls_error(format!("failed to parse private key PEM: {e}")))
}

fn root_store_from_der(certs: Vec<CertificateDer<'static>>) -> AndromedaResult<RootCertStore> {
    if certs.is_empty() {
        return Err(tls_error("mTLS trust root set cannot be empty"));
    }

    let mut roots = RootCertStore::empty();
    for cert in certs {
        roots
            .add(cert)
            .map_err(|e| tls_error(format!("failed to add mTLS trust root: {e}")))?;
    }
    Ok(roots)
}

fn quinn_client_config(cfg: rustls::ClientConfig) -> AndromedaResult<quinn::ClientConfig> {
    let quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(cfg).map_err(|e| {
        AndromedaError::new(
            AndromedaErrorKind::Transport,
            format!("failed to create Quinn client config: {e}"),
        )
    })?;

    Ok(quinn::ClientConfig::new(Arc::new(quic_config)))
}

fn zero_rtt_disabled_server_config(mut config: rustls::ServerConfig) -> rustls::ServerConfig {
    config.max_early_data_size = 0;
    config.send_half_rtt_data = false;
    config
}

fn zero_rtt_disabled_client_config(mut config: rustls::ClientConfig) -> rustls::ClientConfig {
    config.enable_early_data = false;
    config
}

fn tls_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

/// Insecure certificate verifier that accepts all certificates (for testing only).
#[cfg(any(test, feature = "insecure-test-tls"))]
#[derive(Debug)]
struct InsecureVerifier;

#[cfg(any(test, feature = "insecure-test-tls"))]
impl rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
