//! Catalog-facing alias for the workspace-wide SHA-256 backend defined in
//! `andromeda-digest`. Centralising the digest implementation guarantees that
//! catalog hash sinks (`StableHashSink`, `ObjectShapeHashSink`,
//! `PolicyVersion` digests) and protocol-side descriptor hashes all derive
//! from the same FIPS-180-4 implementation without duplication.

pub use andromeda_digest::{Sha256, sha256};

pub(crate) fn digest_prefix_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        out.push(char::from(HEX[(byte >> 4) as usize]));
        out.push(char::from(HEX[(byte & 0x0F) as usize]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_digest_alias_is_workspace_digest_type() {
        let mut catalog_hasher: Sha256 = andromeda_digest::Sha256::new();
        catalog_hasher.update(b"catalog digest alias");

        let mut core_hasher: andromeda_digest::Sha256 = Sha256::new();
        core_hasher.update(b"catalog ");
        core_hasher.update(b"digest alias");

        assert_eq!(catalog_hasher.finalize(), core_hasher.finalize());
    }

    #[test]
    fn catalog_digest_alias_matches_workspace_digest_for_representative_messages() {
        let messages: &[&[u8]] = &[
            b"",
            b"abc",
            b"andromeda.catalog.procedure-contract.v3.sha256",
            b"andromeda.catalog.policy-version.v1.sha256",
        ];

        for message in messages {
            assert_eq!(sha256(message), andromeda_digest::sha256(message));
        }
    }

    #[test]
    fn digest_prefix_hex_is_stable_and_bounded() {
        let digest = [
            0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];

        assert_eq!(digest_prefix_hex(&digest), "abcdef0123456789");
    }
}
