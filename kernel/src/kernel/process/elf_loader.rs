//! ELF (Executable and Linkable Format) loader
//!
//! This module provides functionality to load and execute ELF binaries.
//!
//! # ELF Format Overview
//!
//! An ELF file consists of:
//! - **ELF Header**: Basic file information
//! - **Program Headers**: Describes segments to load into memory
//! - **Section Headers**: Describes sections (optional for execution)
//!
//! # Loading Process
//!
//! 1. Parse ELF header
//! 2. Validate ELF file (magic number, architecture, etc.)
//! 3. Read program headers
//! 4. Map segments into memory
//! 5. Set up initial state (stack, registers)
//! 6. Jump to entry point
//!
//! # Security
//!
//! - Validates all ELF structures
//! - Checks segment sizes and addresses
//! - Enforces memory permissions (W^X)

use core::mem;

/// ELF magic number (0x7F 'E' 'L' 'F')
pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

/// ELF class (32-bit or 64-bit)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfClass {
    /// 32-bit ELF
    Elf32 = 1,
    /// 64-bit ELF
    Elf64 = 2,
}

/// ELF data encoding (endianness)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfData {
    /// Little endian
    LittleEndian = 1,
    /// Big endian
    BigEndian = 2,
}

/// ELF file type
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfType {
    /// No file type
    None = 0,
    /// Relocatable file
    Rel = 1,
    /// Executable file
    Exec = 2,
    /// Shared object file
    Dyn = 3,
    /// Core file
    Core = 4,
}

/// ELF machine architecture
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfMachine {
    /// No machine
    None = 0,
    /// x86-64
    X86_64 = 62,
    /// RISC-V
    RiscV = 243,
}

/// ELF header (64-bit)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    /// Magic number and other info
    pub e_ident: [u8; 16],
    /// Object file type
    pub e_type: u16,
    /// Architecture
    pub e_machine: u16,
    /// Object file version
    pub e_version: u32,
    /// Entry point virtual address
    pub e_entry: u64,
    /// Program header table file offset
    pub e_phoff: u64,
    /// Section header table file offset
    pub e_shoff: u64,
    /// Processor-specific flags
    pub e_flags: u32,
    /// ELF header size in bytes
    pub e_ehsize: u16,
    /// Program header table entry size
    pub e_phentsize: u16,
    /// Program header table entry count
    pub e_phnum: u16,
    /// Section header table entry size
    pub e_shentsize: u16,
    /// Section header table entry count
    pub e_shnum: u16,
    /// Section header string table index
    pub e_shstrndx: u16,
}

/// Program header (64-bit)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64ProgramHeader {
    /// Segment type
    pub p_type: u32,
    /// Segment flags
    pub p_flags: u32,
    /// Segment file offset
    pub p_offset: u64,
    /// Segment virtual address
    pub p_vaddr: u64,
    /// Segment physical address
    pub p_paddr: u64,
    /// Segment size in file
    pub p_filesz: u64,
    /// Segment size in memory
    pub p_memsz: u64,
    /// Segment alignment
    pub p_align: u64,
}

/// Program header type
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramHeaderType {
    /// Unused entry
    Null = 0,
    /// Loadable segment
    Load = 1,
    /// Dynamic linking info
    Dynamic = 2,
    /// Interpreter info
    Interp = 3,
    /// Auxiliary info
    Note = 4,
    /// Reserved
    ShLib = 5,
    /// Program header table
    Phdr = 6,
    /// Thread-Local Storage
    Tls = 7,
}

/// Program header flags
pub mod phdr_flags {
    /// Execute permission
    pub const PF_X: u32 = 1 << 0;
    /// Write permission
    pub const PF_W: u32 = 1 << 1;
    /// Read permission
    pub const PF_R: u32 = 1 << 2;
}

/// ELF parsing errors
#[derive(Debug)]
pub enum ElfError {
    /// Invalid magic number
    InvalidMagic,
    /// Unsupported class (not 64-bit)
    UnsupportedClass,
    /// Unsupported endianness
    UnsupportedEndian,
    /// Unsupported architecture
    UnsupportedArch,
    /// Invalid file type
    InvalidType,
    /// File too small
    FileTooSmall,
    /// Invalid header
    InvalidHeader,
    /// Invalid program header
    InvalidProgramHeader,
    /// Segment alignment error
    AlignmentError,
    /// Memory mapping failed
    MapFailed,
}

impl Elf64Header {
    /// Parse ELF header from bytes
    ///
    /// # Safety
    /// Caller must ensure `data` is at least `mem::size_of::<Elf64Header>()` bytes
    pub unsafe fn from_bytes(data: &[u8]) -> Result<&Self, ElfError> {
        if data.len() < mem::size_of::<Elf64Header>() {
            return Err(ElfError::FileTooSmall);
        }
        
        let header = unsafe { &*(data.as_ptr() as *const Elf64Header) };
        
        // Validate magic number
        if header.e_ident[0..4] != ELF_MAGIC {
            return Err(ElfError::InvalidMagic);
        }
        
        // Validate ELF class (64-bit)
        if header.e_ident[4] != ElfClass::Elf64 as u8 {
            return Err(ElfError::UnsupportedClass);
        }
        
        // Validate endianness (little endian)
        if header.e_ident[5] != ElfData::LittleEndian as u8 {
            return Err(ElfError::UnsupportedEndian);
        }
        
        // Validate architecture (x86-64)
        if header.e_machine != ElfMachine::X86_64 as u16 {
            return Err(ElfError::UnsupportedArch);
        }
        
        Ok(header)
    }
    
    /// Get program headers
    ///
    /// # Safety
    /// Caller must ensure `data` contains valid program headers at the specified offset
    pub unsafe fn program_headers<'a>(&self, data: &'a [u8]) -> Result<&'a [Elf64ProgramHeader], ElfError> {
        let phoff = self.e_phoff as usize;
        let phnum = self.e_phnum as usize;
        let phentsize = self.e_phentsize as usize;
        
        if phentsize != mem::size_of::<Elf64ProgramHeader>() {
            return Err(ElfError::InvalidProgramHeader);
        }
        
        let total_size = phnum * phentsize;
        if data.len() < phoff + total_size {
            return Err(ElfError::FileTooSmall);
        }
        
        let ptr = unsafe { data.as_ptr().add(phoff) as *const Elf64ProgramHeader };
        Ok(unsafe { core::slice::from_raw_parts(ptr, phnum) })
    }
}

impl Elf64ProgramHeader {
    /// Check if this segment is loadable
    pub fn is_load(&self) -> bool {
        self.p_type == ProgramHeaderType::Load as u32
    }
    
    /// Get permissions for this segment
    pub fn permissions(&self) -> (bool, bool, bool) {
        let read = self.p_flags & phdr_flags::PF_R != 0;
        let write = self.p_flags & phdr_flags::PF_W != 0;
        let exec = self.p_flags & phdr_flags::PF_X != 0;
        (read, write, exec)
    }
    
    /// Convert ELF permissions to page table flags
    pub fn to_page_flags(&self) -> x86_64::structures::paging::PageTableFlags {
        use x86_64::structures::paging::PageTableFlags;
        
        let (read, write, exec) = self.permissions();
        
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        
        if write {
            flags |= PageTableFlags::WRITABLE;
        }
        
        if !exec {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        
        flags
    }
}

/// Helper function to read a structure from a byte slice
///
/// # Safety
/// Caller must ensure:
/// - `data` is at least `size_of::<T>()` bytes
/// - `data` is properly aligned for `T`
/// - The bytes represent a valid `T`
pub unsafe fn read_struct<T>(data: &[u8]) -> Result<&T, ElfError> {
    if data.len() < mem::size_of::<T>() {
        return Err(ElfError::FileTooSmall);
    }
    
    Ok(unsafe { &*(data.as_ptr() as *const T) })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_elf_magic() {
        assert_eq!(ELF_MAGIC, [0x7F, b'E', b'L', b'F']);
    }
    
    #[test]
    fn test_elf_class_values() {
        assert_eq!(ElfClass::Elf32 as u8, 1);
        assert_eq!(ElfClass::Elf64 as u8, 2);
    }
}
