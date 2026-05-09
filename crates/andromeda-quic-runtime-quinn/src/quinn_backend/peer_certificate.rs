use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_principal::{CertificateIdentity, SurfaceScope};

use andromeda_quic::mtls_identity::RawCertificate;

pub(super) fn extract_peer_certificates(conn: &quinn::Connection) -> Vec<RawCertificate> {
    let Some(peer_identity) = conn.peer_identity() else {
        return Vec::new();
    };

    let Ok(cert_chain) =
        peer_identity.downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
    else {
        return Vec::new();
    };

    cert_chain
        .iter()
        .map(|cert| RawCertificate::new(cert.as_ref().to_vec()))
        .collect()
}

pub(super) fn require_certificate_identity(
    peer_certificates: &[RawCertificate],
    required_scope: SurfaceScope,
) -> AndromedaResult<CertificateIdentity> {
    let Some(cert) = peer_certificates.first() else {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "QUIC mTLS handshake did not expose an authenticated peer certificate",
        ));
    };

    cert.to_certificate_identity(required_scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_certificate_identity_rejects_missing_peer_certificate() {
        let err = require_certificate_identity(&[], SurfaceScope::Application).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Security);
        assert!(
            err.message().contains("mTLS"),
            "missing peer certificate must fail closed as mTLS security evidence"
        );
    }
}
