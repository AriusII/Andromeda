//! TLS configuration for QUIC server and client.
//!
//! This module handles certificate generation (ephemeral or from disk) and
//! TLS config construction for both server and client sides.

use std::path::Path;
use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject},
};

/// TLS configuration builder for QUIC server.
pub struct ServerTlsConfig {
    config: ServerConfig,
}

impl ServerTlsConfig {
    /// Creates a TLS config from certificate and private key files.
    ///
    /// # Arguments
    /// - `cert_path`: Path to PEM-encoded certificate file
    /// - `key_path`: Path to PEM-encoded private key file
    pub fn from_files(cert_path: &Path, key_path: &Path) -> AndromedaResult<Self> {
        let cert_pem = std::fs::read_to_string(cert_path).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to read certificate file: {}", e),
            )
        })?;

        let key_pem = std::fs::read_to_string(key_path).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Transport,
                format!("failed to read private key file: {}", e),
            )
        })?;

        let certs = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    format!("failed to parse certificate PEM: {}", e),
                )
            })?;

        if certs.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "no certificates found in PEM file".to_string(),
            ));
        }

        let key = PrivatePkcs8KeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Protocol,
                format!("failed to parse private key PEM: {}", e),
            )
        })?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, PrivateKeyDer::Pkcs8(key))
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    format!("failed to build server config: {}", e),
                )
            })?;

        Ok(Self { config })
    }

    /// Creates an ephemeral self-signed certificate for testing.
    ///
    /// Generates a new certificate valid for the given domain names.
    pub fn ephemeral(subject_alt_names: Vec<String>) -> AndromedaResult<Self> {
        use rcgen::generate_simple_self_signed;

        let rcgen::CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(subject_alt_names).map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    format!("failed to generate self-signed certificate: {}", e),
                )
            })?;

        let cert_der = cert.der().clone();
        let key_der = signing_key.serialize_der();

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_der],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der)),
            )
            .map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    format!("failed to build server config: {}", e),
                )
            })?;

        Ok(Self { config })
    }

    /// Returns the configured rustls ServerConfig.
    pub fn into_quinn_config(self) -> quinn::ServerConfig {
        quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(self.config)
                .expect("failed to create Quinn server config"),
        ))
    }
}

/// Helper for creating insecure client TLS configuration (for testing only).
pub struct ClientTlsConfig;

impl ClientTlsConfig {
    /// Creates a client TLS config that accepts all server certificates (for testing).
    ///
    /// **WARNING**: This disables certificate verification. Use only in tests!
    pub fn insecure() -> quinn::ClientConfig {
        // For testing with self-signed certs, we create a client config
        // that will accept any certificate
        let cfg = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
            .with_no_client_auth();

        quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(cfg)
                .expect("failed to create Quinn client config"),
        ))
    }
}

/// Insecure certificate verifier that accepts all certificates (for testing only).
#[derive(Debug)]
struct InsecureVerifier;

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
