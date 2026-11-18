//! Embedded network asset.
//!
//! The network is compiled into the binary so the release artifact has no
//! external data dependency. The checksum test protects against an accidental
//! file replacement during development.

/// Size of the embedded network file in bytes.
pub const NET_SIZE: usize = 6_294_672;

/// Name under which the embedded network announces itself in UCI output
/// (`eval` and the eval-source line): the file it was built from.
pub const NET_SOURCE_NAME: &str = "jokausch.nnue";

/// SHA-256 digest of the embedded network file.
pub const NET_SHA256: &str = "0a70169ce7d75a9a5a392cd293746fc1f5191a75dc183b5b45fb69d869ce2968";

/// Returns the embedded network bytes.
pub fn bytes() -> &'static [u8] {
    include_bytes!("../../net/jokausch.nnue")
}

#[cfg(test)]
mod tests {
    use super::{NET_SHA256, NET_SIZE, bytes};
    use sha2::{Digest, Sha256};

    #[test]
    fn embedded_network_has_expected_size() {
        assert_eq!(bytes().len(), NET_SIZE);
    }

    #[test]
    fn embedded_network_has_expected_checksum() {
        let digest = Sha256::digest(bytes());
        let checksum: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(checksum, NET_SHA256);
    }
}
