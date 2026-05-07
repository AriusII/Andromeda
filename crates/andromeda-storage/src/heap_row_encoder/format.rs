pub(crate) const BITS_PER_BITMAP_BYTE: usize = 8;
pub(crate) const VAR_OFFSET_WIDTH_BYTES: usize = 4;

pub(crate) fn bitmap_byte_len(bit_count: usize) -> usize {
    bit_count.div_ceil(BITS_PER_BITMAP_BYTE)
}
