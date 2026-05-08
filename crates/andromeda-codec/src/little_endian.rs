use std::ops::Range;

use crate::{CodecBoundsError, CodecError, CodecResult};

pub fn read_exact<const N: usize>(input: &[u8], offset: usize) -> CodecResult<([u8; N], usize)> {
    let range = checked_range(input.len(), offset, N, BoundsDirection::Read)?;
    let mut bytes = [0_u8; N];
    bytes.copy_from_slice(&input[range.clone()]);
    Ok((bytes, range.end))
}

pub fn write_exact<const N: usize>(
    output: &mut [u8],
    offset: usize,
    bytes: [u8; N],
) -> CodecResult<usize> {
    let range = checked_range(output.len(), offset, N, BoundsDirection::Write)?;
    output[range.clone()].copy_from_slice(&bytes);
    Ok(range.end)
}

pub fn read_u8(input: &[u8], offset: usize) -> CodecResult<(u8, usize)> {
    let (bytes, next_offset) = read_exact::<1>(input, offset)?;
    Ok((bytes[0], next_offset))
}

pub fn read_u16_le(input: &[u8], offset: usize) -> CodecResult<(u16, usize)> {
    let (bytes, next_offset) = read_exact::<2>(input, offset)?;
    Ok((u16::from_le_bytes(bytes), next_offset))
}

pub fn read_u32_le(input: &[u8], offset: usize) -> CodecResult<(u32, usize)> {
    let (bytes, next_offset) = read_exact::<4>(input, offset)?;
    Ok((u32::from_le_bytes(bytes), next_offset))
}

pub fn read_u64_le(input: &[u8], offset: usize) -> CodecResult<(u64, usize)> {
    let (bytes, next_offset) = read_exact::<8>(input, offset)?;
    Ok((u64::from_le_bytes(bytes), next_offset))
}

pub fn read_u128_le(input: &[u8], offset: usize) -> CodecResult<(u128, usize)> {
    let (bytes, next_offset) = read_exact::<16>(input, offset)?;
    Ok((u128::from_le_bytes(bytes), next_offset))
}

pub fn write_u8(output: &mut [u8], offset: usize, value: u8) -> CodecResult<usize> {
    write_exact(output, offset, value.to_le_bytes())
}

pub fn write_u16_le(output: &mut [u8], offset: usize, value: u16) -> CodecResult<usize> {
    write_exact(output, offset, value.to_le_bytes())
}

pub fn write_u32_le(output: &mut [u8], offset: usize, value: u32) -> CodecResult<usize> {
    write_exact(output, offset, value.to_le_bytes())
}

pub fn write_u64_le(output: &mut [u8], offset: usize, value: u64) -> CodecResult<usize> {
    write_exact(output, offset, value.to_le_bytes())
}

pub fn write_u128_le(output: &mut [u8], offset: usize, value: u128) -> CodecResult<usize> {
    write_exact(output, offset, value.to_le_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundsDirection {
    Read,
    Write,
}

fn checked_range(
    len: usize,
    offset: usize,
    needed: usize,
    direction: BoundsDirection,
) -> CodecResult<Range<usize>> {
    let end = offset
        .checked_add(needed)
        .ok_or(CodecError::OffsetOverflow { offset, needed })?;

    if end > len {
        let bounds = CodecBoundsError::new(offset, needed, len.saturating_sub(offset));
        return match direction {
            BoundsDirection::Read => Err(CodecError::ReadOutOfBounds(bounds)),
            BoundsDirection::Write => Err(CodecError::WriteOutOfBounds(bounds)),
        };
    }

    Ok(offset..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn little_endian_helpers_roundtrip_unsigned_values() {
        let mut bytes = [0_u8; 31];
        let mut offset = 0;

        offset = write_u8(&mut bytes, offset, 0xab).unwrap();
        offset = write_u16_le(&mut bytes, offset, 0x1234).unwrap();
        offset = write_u32_le(&mut bytes, offset, 0x1234_5678).unwrap();
        offset = write_u64_le(&mut bytes, offset, 0x0102_0304_0506_0708).unwrap();
        offset = write_u128_le(
            &mut bytes,
            offset,
            0x0102_0304_0506_0708_1112_1314_1516_1718,
        )
        .unwrap();

        assert_eq!(offset, bytes.len());
        assert_eq!(&bytes[0..3], &[0xab, 0x34, 0x12]);

        let (u8_value, offset) = read_u8(&bytes, 0).unwrap();
        let (u16_value, offset) = read_u16_le(&bytes, offset).unwrap();
        let (u32_value, offset) = read_u32_le(&bytes, offset).unwrap();
        let (u64_value, offset) = read_u64_le(&bytes, offset).unwrap();
        let (u128_value, offset) = read_u128_le(&bytes, offset).unwrap();

        assert_eq!(u8_value, 0xab);
        assert_eq!(u16_value, 0x1234);
        assert_eq!(u32_value, 0x1234_5678);
        assert_eq!(u64_value, 0x0102_0304_0506_0708);
        assert_eq!(u128_value, 0x0102_0304_0506_0708_1112_1314_1516_1718);
        assert_eq!(offset, bytes.len());
    }

    #[test]
    fn read_bounds_report_remaining_bytes() {
        let error = read_u32_le(&[1, 2, 3], 1).unwrap_err();

        assert_eq!(
            error,
            CodecError::ReadOutOfBounds(CodecBoundsError::new(1, 4, 2))
        );
    }

    #[test]
    fn write_bounds_report_remaining_bytes() {
        let mut output = [0_u8; 3];
        let error = write_u64_le(&mut output, 2, 9).unwrap_err();

        assert_eq!(
            error,
            CodecError::WriteOutOfBounds(CodecBoundsError::new(2, 8, 1))
        );
        assert_eq!(output, [0, 0, 0]);
    }

    #[test]
    fn cursor_overflow_is_distinct_from_slice_bounds() {
        let error = read_u16_le(&[], usize::MAX).unwrap_err();

        assert_eq!(
            error,
            CodecError::OffsetOverflow {
                offset: usize::MAX,
                needed: 2
            }
        );
    }
}
