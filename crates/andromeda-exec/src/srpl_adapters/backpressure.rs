use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Stream backpressure controller.
#[derive(Debug, Clone)]
pub struct SrplStreamBackpressure {
    max_buffered_rows: usize,
    buffered_rows: usize,
}

impl SrplStreamBackpressure {
    pub fn new(max_buffered_rows: usize) -> Self {
        Self {
            max_buffered_rows,
            buffered_rows: 0,
        }
    }

    pub fn buffer_rows(&mut self, count: usize) -> AndromedaResult<()> {
        if self.buffered_rows + count > self.max_buffered_rows {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                format!(
                    "SRPL stream backpressure exceeded: {} rows buffered, max {}",
                    self.buffered_rows + count,
                    self.max_buffered_rows
                ),
            ));
        }
        self.buffered_rows += count;
        Ok(())
    }

    pub fn release_rows(&mut self, count: usize) {
        self.buffered_rows = self.buffered_rows.saturating_sub(count);
    }
}
