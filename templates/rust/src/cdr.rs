//! Allocation-free little-endian CDRv1 primitive codec.

pub const LE_ENCAPSULATION: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdrError {
    BufferTooSmall,
    InvalidEncapsulation,
    TrailingBytes,
    SizeMismatch,
}

pub struct CdrWriter<'a> {
    bytes: &'a mut [u8],
    position: usize,
    alignment_origin: usize,
}

impl<'a> CdrWriter<'a> {
    pub fn new_le(bytes: &'a mut [u8]) -> Result<Self, CdrError> {
        if bytes.len() < LE_ENCAPSULATION.len() {
            return Err(CdrError::BufferTooSmall);
        }
        bytes[..LE_ENCAPSULATION.len()].copy_from_slice(&LE_ENCAPSULATION);
        Ok(Self {
            bytes,
            position: LE_ENCAPSULATION.len(),
            alignment_origin: LE_ENCAPSULATION.len(),
        })
    }

    fn reserve(&mut self, alignment: usize, size: usize) -> Result<(), CdrError> {
        let relative = self.position - self.alignment_origin;
        let padding = (alignment - (relative & (alignment - 1))) & (alignment - 1);
        let end = self
            .position
            .checked_add(padding)
            .and_then(|position| position.checked_add(size))
            .ok_or(CdrError::BufferTooSmall)?;
        if end > self.bytes.len() {
            return Err(CdrError::BufferTooSmall);
        }
        self.bytes[self.position..self.position + padding].fill(0);
        self.position += padding;
        Ok(())
    }

    pub fn write_u8(&mut self, value: u8) -> Result<(), CdrError> {
        self.reserve(1, 1)?;
        self.bytes[self.position] = value;
        self.position += 1;
        Ok(())
    }

    pub fn write_i16(&mut self, value: i16) -> Result<(), CdrError> {
        self.write_bytes_aligned(&value.to_le_bytes(), 2)
    }

    pub fn write_u16(&mut self, value: u16) -> Result<(), CdrError> {
        self.write_bytes_aligned(&value.to_le_bytes(), 2)
    }

    pub fn write_i32(&mut self, value: i32) -> Result<(), CdrError> {
        self.write_bytes_aligned(&value.to_le_bytes(), 4)
    }

    pub fn write_u32(&mut self, value: u32) -> Result<(), CdrError> {
        self.write_bytes_aligned(&value.to_le_bytes(), 4)
    }

    pub fn write_u64(&mut self, value: u64) -> Result<(), CdrError> {
        self.write_bytes_aligned(&value.to_le_bytes(), 8)
    }

    pub fn write_f32(&mut self, value: f32) -> Result<(), CdrError> {
        self.write_u32(value.to_bits())
    }

    fn write_bytes_aligned(
        &mut self,
        value: &[u8],
        alignment: usize,
    ) -> Result<(), CdrError> {
        self.reserve(alignment, value.len())?;
        let end = self.position + value.len();
        self.bytes[self.position..end].copy_from_slice(value);
        self.position = end;
        Ok(())
    }

    pub fn finish_exact(self, expected_size: usize) -> Result<usize, CdrError> {
        if self.position != expected_size {
            return Err(CdrError::SizeMismatch);
        }
        Ok(self.position)
    }
}

pub struct CdrReader<'a> {
    bytes: &'a [u8],
    position: usize,
    alignment_origin: usize,
}

impl<'a> CdrReader<'a> {
    pub fn new_le(bytes: &'a [u8]) -> Result<Self, CdrError> {
        if bytes.len() < LE_ENCAPSULATION.len() {
            return Err(CdrError::BufferTooSmall);
        }
        if bytes[..LE_ENCAPSULATION.len()] != LE_ENCAPSULATION {
            return Err(CdrError::InvalidEncapsulation);
        }
        Ok(Self {
            bytes,
            position: LE_ENCAPSULATION.len(),
            alignment_origin: LE_ENCAPSULATION.len(),
        })
    }

    fn take<const N: usize>(&mut self, alignment: usize) -> Result<[u8; N], CdrError> {
        let relative = self.position - self.alignment_origin;
        let padding = (alignment - (relative & (alignment - 1))) & (alignment - 1);
        let start = self
            .position
            .checked_add(padding)
            .ok_or(CdrError::BufferTooSmall)?;
        let end = start.checked_add(N).ok_or(CdrError::BufferTooSmall)?;
        let slice = self
            .bytes
            .get(start..end)
            .ok_or(CdrError::BufferTooSmall)?;
        self.position = end;
        Ok(slice.try_into().expect("fixed-size slice"))
    }

    pub fn read_u8(&mut self) -> Result<u8, CdrError> {
        Ok(self.take::<1>(1)?[0])
    }

    pub fn read_i16(&mut self) -> Result<i16, CdrError> {
        Ok(i16::from_le_bytes(self.take::<2>(2)?))
    }

    pub fn read_u16(&mut self) -> Result<u16, CdrError> {
        Ok(u16::from_le_bytes(self.take::<2>(2)?))
    }

    pub fn read_i32(&mut self) -> Result<i32, CdrError> {
        Ok(i32::from_le_bytes(self.take::<4>(4)?))
    }

    pub fn read_u32(&mut self) -> Result<u32, CdrError> {
        Ok(u32::from_le_bytes(self.take::<4>(4)?))
    }

    pub fn read_u64(&mut self) -> Result<u64, CdrError> {
        Ok(u64::from_le_bytes(self.take::<8>(8)?))
    }

    pub fn read_f32(&mut self) -> Result<f32, CdrError> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    pub fn finish(self) -> Result<(), CdrError> {
        if self.position != self.bytes.len() {
            return Err(CdrError::TrailingBytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_is_relative_to_the_cdr_body() {
        let mut bytes = [0xff; 16];
        let mut writer = CdrWriter::new_le(&mut bytes).unwrap();
        writer.write_u8(7).unwrap();
        writer.write_u32(0x11223344).unwrap();
        assert_eq!(writer.finish_exact(12), Ok(12));
        assert_eq!(
            &bytes[..12],
            &[0, 1, 0, 0, 7, 0, 0, 0, 0x44, 0x33, 0x22, 0x11]
        );
    }

    #[test]
    fn reader_rejects_wrong_encapsulation_and_trailing_bytes() {
        assert_eq!(
            CdrReader::new_le(&[0, 0, 0, 0]).err(),
            Some(CdrError::InvalidEncapsulation)
        );
        let reader = CdrReader::new_le(&[0, 1, 0, 0, 0]).unwrap();
        assert_eq!(reader.finish(), Err(CdrError::TrailingBytes));
    }
}
