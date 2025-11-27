use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use walkdir::WalkDir;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: mkcpio <source_dir> <output_file>");
        std::process::exit(1);
    }

    let source_dir = &args[1];
    let output_file = &args[2];

    let mut out = File::create(output_file)?;
    let mut inode = 1;

    println!("Creating CPIO archive: {} -> {}", source_dir, output_file);

    for entry in WalkDir::new(source_dir) {
        let entry = entry.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let path = entry.path();
        
        // Skip the root directory itself if it matches source_dir
        if path == Path::new(source_dir) {
            continue;
        }

        let metadata = path.metadata()?;
        
        // Relative path for cpio header
        let rel_path = path.strip_prefix(source_dir)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
            .to_str()
            .ok_or(io::Error::new(io::ErrorKind::Other, "Invalid UTF-8 path"))?
            .replace("\\", "/"); // Normalize to forward slashes

        // Basic file type detection
        let mode = if metadata.is_dir() {
            0o40755 // Directory
        } else {
            0o100755 // Regular file (executable)
        };

        let size = if metadata.is_dir() { 0 } else { metadata.len() };

        println!("  Adding: {} (mode={:o}, size={})", rel_path, mode, size);

        write_cpio_header(&mut out, &rel_path, mode, size as u32, inode)?;
        inode += 1;

        if !metadata.is_dir() {
            let mut file = File::open(path)?;
            let copied = io::copy(&mut file, &mut out)?;
            
            let padding = (4 - (copied % 4)) % 4;
            for _ in 0..padding {
                out.write_all(&[0])?;
            }
        }
    }

    // Write TRAILER!!!
    write_cpio_header(&mut out, "TRAILER!!!", 0, 0, 0)?;

    Ok(())
}

fn write_cpio_header<W: Write>(out: &mut W, name: &str, mode: u32, size: u32, inode: u32) -> io::Result<()> {
    let header_size = 110;
    let name_len = name.len() + 1;
    
    // Newc format
    write!(out, "070701")?;
    write!(out, "{:08X}", inode)?; 
    write!(out, "{:08X}", mode)?;
    write!(out, "{:08X}", 0)?; // UID
    write!(out, "{:08X}", 0)?; // GID
    write!(out, "{:08X}", 1)?; // Nlink
    write!(out, "{:08X}", 0)?; // Mtime
    write!(out, "{:08X}", size)?;
    write!(out, "{:08X}", 0)?; // DevMajor
    write!(out, "{:08X}", 0)?; // DevMinor
    write!(out, "{:08X}", 0)?; // RdevMajor
    write!(out, "{:08X}", 0)?; // RdevMinor
    write!(out, "{:08X}", name_len)?;
    write!(out, "{:08X}", 0)?; // Checksum

    out.write_all(name.as_bytes())?;
    out.write_all(&[0])?; 

    let total_written = header_size + name_len;
    let padding = (4 - (total_written % 4)) % 4;
    for _ in 0..padding {
        out.write_all(&[0])?;
    }
    
    Ok(())
}
