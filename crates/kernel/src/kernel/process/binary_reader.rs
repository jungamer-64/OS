// kernel/src/kernel/process/binary_reader.rs
//! Binary reader utilities for ELF loading
//!
//! Provides safe abstractions for reading binary data from byte slices.

/// Binary reader for safely reading structured data
pub struct BinaryReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> BinaryReader<'a> {
    /// Create a new binary reader
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
    
    /// Get current offset
    pub const fn offset(&self) -> usize {
        self.offset
    }
    
    /// Get remaining bytes
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }
    
    /// Check if there are enough bytes remaining
    pub fn has_bytes(&self, count: usize) -> bool {
        self.remaining() >= count
    }
    
    /// Read a u8
    pub fn read_u8(&mut self) -> Option<u8> {
        if self.has_bytes(1) {
            let val = self.data[self.offset];
            self.offset += 1;
            Some(val)
        } else {
            None
        }
    }
    
    /// Read a u16 (little endian)
    pub fn read_u16(&mut self) -> Option<u16> {
        if self.has_bytes(2) {
            let bytes = [
                self.data[self.offset],
                self.data[self.offset + 1],
            ];
            self.offset += 2;
            Some(u16::from_le_bytes(bytes))
        } else {
            None
        }
    }
    
    /// Read a u32 (little endian)
    pub fn read_u32(&mut self) -> Option<u32> {
        if self.has_bytes(4) {
            let bytes = [
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
            ];
            self.offset += 4;
            Some(u32::from_le_bytes(bytes))
        } else {
            None
        }
    }
    
    /// Read a u64 (little endian)
    pub fn read_u64(&mut self) -> Option<u64> {
        if self.has_bytes(8) {
            let bytes = [
                self.data[self.offset],
                self.data[self.offset + 1],
                self.data[self.offset + 2],
                self.data[self.offset + 3],
                self.data[self.offset + 4],
                self.data[self.offset + 5],
                self.data[self.offset + 6],
                self.data[self.offset + 7],
            ];
            self.offset += 8;
            Some(u64::from_le_bytes(bytes))
        } else {
            None
        }
    }
    
    /// Read bytes into a slice
    pub fn read_bytes(&mut self, out: &mut [u8]) -> Option<()> {
        if self.has_bytes(out.len()) {
            out.copy_from_slice(&self.data[self.offset..self.offset + out.len()]);
            self.offset += out.len();
            Some(())
        } else {
            None
        }
    }
    
    /// Seek to absolute offset
    pub fn seek(&mut self, offset: usize) -> Result<(), ()> {
        if offset <= self.data.len() {
            self.offset = offset;
            Ok(())
        } else {
            Err(())
        }
    }
    
    /// Get a slice of remaining data
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.offset..]
    }
    
    /// Peek at bytes without advancing
    pub fn peek_bytes(&self, count: usize) -> Option<&'a [u8]> {
        if self.has_bytes(count) {
            Some(&self.data[self.offset..self.offset + count])
        } else {
            None
        }
    }
    
    /// Skip bytes
    pub fn skip(&mut self, count: usize) -> Result<(), ()> {
        if self.has_bytes(count) {
            self.offset += count;
            Ok(())
        } else {
            Err(())
        }
    }
    
    /// Align offset to specified boundary
    pub fn align(&mut self, alignment: usize) -> Result<(), ()> {
        let remainder = self.offset % alignment;
        if remainder != 0 {
            let padding = alignment - remainder;
            self.skip(padding)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_read_u8() {
        let data = [0x12, 0x34, 0x56];
        let mut reader = BinaryReader::new(&data);
        assert_eq!(reader.read_u8(), Some(0x12));
        assert_eq!(reader.read_u8(), Some(0x34));
        assert_eq!(reader.read_u8(), Some(0x56));
        assert_eq!(reader.read_u8(), None);
    }
    
    #[test]
    fn test_read_u32() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut reader = BinaryReader::new(&data);
        assert_eq!(reader.read_u32(), Some(0x78563412)); // Little endian
    }
    
    #[test]
    fn test_seek() {
        let data = [0,  1, 2, 3, 4];
        let mut reader = BinaryReader::new(&data);
        assert_eq!(reader.seek(2), Ok(()));
        assert_eq!(reader.read_u8(), Some(2));
    }
    
    #[test]
    fn test_align() {
        let data = [0; 10];
        let mut reader = BinaryReader::new(&data);
        reader.skip(3).unwrap();
        assert_eq!(reader.offset(), 3);
        reader.align(4).unwrap();
        assert_eq!(reader.offset(), 4);
    }
}
