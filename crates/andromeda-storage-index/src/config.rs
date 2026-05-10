/// Configuration for B-Tree index operations.
#[derive(Debug, Clone)]
pub struct BTreeConfig {
    pub branching_factor: u16,
    pub max_key_size: u32,
    pub max_tree_height: u32,
}

impl Default for BTreeConfig {
    fn default() -> Self {
        Self {
            branching_factor: 128,
            max_key_size: 4096,
            max_tree_height: 32,
        }
    }
}

/// DEC-032/DEC-038 guardrail for page-backed B-Tree durability.
///
/// This remains `false` until the node image, WAL mutation payloads, recovery
/// replay, crash tests, golden vectors, and fuzz coverage are promoted as one
/// contract. Validating `BTreeNodeV1` decode is necessary but not sufficient:
/// visible durable mutation still requires WAL-covered split/merge/insert/delete
/// semantics and idempotent recovery before this gate can open.
pub const BTREE_DURABLE_FORMAT_PROMOTED: bool = false;

#[cfg(test)]
mod tests {
    use super::{BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig};

    #[test]
    fn default_btree_config_preserves_storage_contract_limits() {
        let config = BTreeConfig::default();

        assert_eq!(config.branching_factor, 128);
        assert_eq!(config.max_key_size, 4096);
        assert_eq!(config.max_tree_height, 32);
    }

    #[test]
    fn durable_btree_format_remains_fail_stop_until_promotion() {
        const {
            assert!(!BTREE_DURABLE_FORMAT_PROMOTED);
        }
    }
}
