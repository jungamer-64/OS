// kernel/src/kernel/fs/initrd.rs
use super::FileSystem;
use core::str;

/// CPIO Newc format magic
const CPIO_MAGIC: &str = "070701";

/// Initrd filesystem (Read-only CPIO archive)
pub struct InitrdFs {
    data: &'static [u8],
}

impl InitrdFs {
    /// Create a new InitrdFs from a memory slice
    /// 
    /// # Safety
    /// The caller must ensure that the data slice is valid for the lifetime of the filesystem.
    pub unsafe fn new(data: &'static [u8]) -> Self {
        Self { data }
    }
    
    fn parse_hex(s: &[u8]) -> Option<u32> {
        let s = str::from_utf8(s).ok()?;
        u32::from_str_radix(s, 16).ok()
    }
}

impl FileSystem for InitrdFs {
    fn read_file(&self, path: &str) -> Option<&[u8]> {
        let mut cursor = 0;
        
        // Normalize path (remove leading /)
        let target_path = path.trim_start_matches('/');
        
        while cursor + 110 <= self.data.len() {
            // Check magic
            if &self.data[cursor..cursor+6] != CPIO_MAGIC.as_bytes() {
                break;
            }
            
            // Parse header fields
            // Namesize is at offset 94, length 8
            let namesize = Self::parse_hex(&self.data[cursor+94..cursor+102])? as usize;
            // Filesize is at offset 54, length 8
            let filesize = Self::parse_hex(&self.data[cursor+54..cursor+62])? as usize;
            
            // Header size is 110
            let header_end = cursor + 110;
            
            // Read filename
            if header_end + namesize > self.data.len() {
                break;
            }
            
            // namesize includes null terminator
            let filename_bytes = &self.data[header_end..header_end+namesize-1]; 
            let filename = str::from_utf8(filename_bytes).ok()?;
            
            // Calculate padding for header + name
            // The header + filename is padded to 4 byte boundary
            let total_header_size = 110 + namesize;
            let name_pad = (4 - (total_header_size % 4)) % 4;
            let content_start = total_header_size + name_pad + cursor;
            
            if filename == "TRAILER!!!" {
                break;
            }
            
            if filename == target_path {
                if content_start + filesize > self.data.len() {
                    return None;
                }
                return Some(&self.data[content_start..content_start+filesize]);
            }
            
            // Skip to next file
            // File content is padded to 4 byte boundary
            let content_pad = (4 - (filesize % 4)) % 4;
            cursor = content_start + filesize + content_pad;
        }
        
        None
    }
}
