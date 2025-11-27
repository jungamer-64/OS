use super::FileSystem;
use spin::Mutex;
use alloc::boxed::Box;
use lazy_static::lazy_static;

/// Virtual Filesystem
pub struct Vfs {
    root: Option<Box<dyn FileSystem + Send + Sync>>,
}

impl Vfs {
    /// Create a new VFS
    pub fn new() -> Self {
        Self { root: None }
    }
    
    /// Mount a filesystem
    /// 
    /// Currently only supports mounting at root "/"
    pub fn mount(&mut self, _path: &str, fs: impl FileSystem + 'static + Send + Sync) {
        self.root = Some(Box::new(fs));
    }
    
    /// Read a file
    pub fn read_file(&self, path: &str) -> Option<&[u8]> {
        if let Some(fs) = &self.root {
            fs.read_file(path)
        } else {
            None
        }
    }
}

lazy_static! {
    /// Global VFS instance
    pub static ref VFS: Mutex<Vfs> = Mutex::new(Vfs::new());
}
