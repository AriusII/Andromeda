//! Catalog-facing alias for the workspace-wide SHA-256 backend defined in
//! `andromeda-core`.  Centralising the digest implementation guarantees that
//! catalog hash sinks (`StableHashSink`, `ObjectShapeHashSink`,
//! `PolicyVersion` digests) and protocol-side descriptor hashes all derive
//! from the same FIPS-180-4 implementation without duplication.

pub use andromeda_core::digest::{sha256, Sha256};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_digest_alias_is_core_digest_type() {
        let mut catalog_hasher: Sha256 = andromeda_core::digest::Sha256::new();
        catalog_hasher.update(b"catalog digest alias");

        let mut core_hasher: andromeda_core::digest::Sha256 = Sha256::new();
        core_hasher.update(b"catalog ");
        core_hasher.update(b"digest alias");

        assert_eq!(catalog_hasher.finalize(), core_hasher.finalize());
    }

    #[test]
    fn catalog_digest_alias_matches_core_digest_for_representative_messages() {
        let messages: &[&[u8]] = &[
            b"",
            b"abc",
            b"andromeda.catalog.procedure-contract.v3.sha256",
            b"andromeda.catalog.policy-version.v1.sha256",
        ];

        for message in messages {
            assert_eq!(sha256(message), andromeda_core::digest::sha256(message));
        }
    }
}
