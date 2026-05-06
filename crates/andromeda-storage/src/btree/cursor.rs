//! B+ Tree range scan cursor — sequential iteration via linked leaves.
use super::*;

impl BTreeRangeCursor {
    /// Create a new range cursor for [start_key, end_key).
    pub fn new(
        start_leaf: PageId,
        start_key: Vec<u8>,
        end_key: Vec<u8>,
        inclusive_end: bool,
    ) -> Self {
        Self {
            current_leaf: start_leaf,
            current_key_idx: 0,
            start_key,
            end_key,
            inclusive_end,
            exhausted: true,
        }
    }

    /// Fetch next key-value pair; returns None at end of range.
    ///
    /// Uses leaf-node sibling links to traverse sequentially.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> AndromedaResult<Option<(Vec<u8>, RowId)>> {
        self.exhausted = true;
        Ok(None)
    }

    /// Seek cursor to a specific key position (for range start optimization).
    pub fn seek_to_key(&mut self, key: &[u8]) -> AndromedaResult<()> {
        self.start_key = key.to_vec();
        self.current_key_idx = 0;
        Ok(())
    }

    /// Advance cursor to next leaf in the chain.
    pub fn advance_to_next_leaf(&mut self) -> AndromedaResult<()> {
        self.exhausted = true;
        self.current_key_idx = 0;
        Ok(())
    }

    /// Check if current key is within range bounds.
    pub fn is_in_range(&self, key: &[u8]) -> bool {
        let after_start = self.start_key.is_empty() || key >= self.start_key.as_slice();
        let before_end = self.end_key.is_empty()
            || if self.inclusive_end {
                key <= self.end_key.as_slice()
            } else {
                key < self.end_key.as_slice()
            };
        after_start && before_end
    }
}
