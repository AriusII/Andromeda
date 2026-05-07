//! B+ Tree leaf node operations — key insertion, deletion, value lookup.
use super::*;

impl BTreeNodeImpl {
    /// Insert a key-value pair into leaf node (maintaining sorted order)
    pub fn insert_into_leaf(&mut self, key: Vec<u8>, row_id: RowId) -> AndromedaResult<()> {
        if !self.is_leaf {
            return Err(BTreeError::CorruptedNode {
                node_id: self.page_id,
                reason: "attempted insert_into_leaf on internal node".to_string(),
            }
            .into());
        }

        let idx = self
            .key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key.as_slice()))
            .unwrap_or_else(|idx| idx);

        // Check for duplicate
        if idx < self.key_value_pairs.len() && self.key_value_pairs[idx].key == key {
            return Err(BTreeError::DuplicateKey { key }.into());
        }

        let row_id_bytes = row_id.get().to_le_bytes().to_vec();
        self.key_value_pairs.insert(
            idx,
            KeyValuePair {
                key,
                value: row_id_bytes,
            },
        );

        Ok(())
    }

    /// Look up a key in leaf node; returns RowId if found
    pub fn lookup_in_leaf(&self, key: &[u8]) -> Option<RowId> {
        if !self.is_leaf {
            return None;
        }

        self.key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
            .ok()
            .and_then(|idx| {
                if let Ok(row_id_value) = <[u8; 8]>::try_from(&self.key_value_pairs[idx].value[..])
                {
                    Some(RowId::new(u64::from_le_bytes(row_id_value)))
                } else {
                    None
                }
            })
    }

    /// Delete a key from leaf node
    pub fn delete_from_leaf(&mut self, key: &[u8]) -> AndromedaResult<()> {
        if !self.is_leaf {
            return Err(BTreeError::CorruptedNode {
                node_id: self.page_id,
                reason: "delete_from_leaf called on internal node".to_string(),
            }
            .into());
        }

        if let Ok(idx) = self
            .key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
        {
            self.key_value_pairs.remove(idx);
            Ok(())
        } else {
            Err(BTreeError::KeyNotFound { key: key.to_vec() }.into())
        }
    }
}
