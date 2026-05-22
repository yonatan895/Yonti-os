use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// A file containing raw byte data, or a directory containing child inodes.
#[derive(Debug, Clone)]
pub enum InodeKind {
    File(Vec<u8>),
    Directory(BTreeMap<String, Inode>),
}

/// A node in the filesystem tree, either a file or a directory.
#[derive(Debug, Clone)]
pub struct Inode {
    pub name: String,
    pub kind: InodeKind,
}

impl Inode {
    /// Create a new empty file inode.
    pub fn new_file(name: &str) -> Self {
        Inode {
            name: String::from(name),
            kind: InodeKind::File(Vec::new()),
        }
    }

    /// Create a new empty directory inode.
    pub fn new_directory(name: &str) -> Self {
        Inode {
            name: String::from(name),
            kind: InodeKind::Directory(BTreeMap::new()),
        }
    }

    /// Return a reference to the file data, or `None` if this is a directory.
    pub fn read(&self) -> Option<&[u8]> {
        match &self.kind {
            InodeKind::File(data) => Some(data),
            InodeKind::Directory(_) => None,
        }
    }

    /// Replace the file contents with the given data.
    ///
    /// Returns an error if this inode is a directory.
    pub fn write(&mut self, data: &[u8]) -> Result<(), &'static str> {
        match &mut self.kind {
            InodeKind::File(ref mut buf) => {
                *buf = Vec::from(data);
                Ok(())
            }
            InodeKind::Directory(_) => Err("cannot write data to a directory"),
        }
    }

    pub fn append(&mut self, data: &[u8]) -> Result<(), &'static str> {
        match &mut self.kind {
            InodeKind::File(ref mut buf) => {
                buf.extend_from_slice(data);
                Ok(())
            }
            InodeKind::Directory(_) => Err("cannot append data to a directory"),
        }
    }

    pub fn size(&self) -> usize {
        match &self.kind {
            InodeKind::File(data) => data.len(),
            InodeKind::Directory(children) => children.len(),
        }
    }

    pub fn find_child(&self, name: &str) -> Option<&Inode> {
        match &self.kind {
            InodeKind::Directory(children) => children.get(name),
            InodeKind::File(_) => None,
        }
    }

    pub fn find_child_mut(&mut self, name: &str) -> Option<&mut Inode> {
        match &mut self.kind {
            InodeKind::Directory(children) => children.get_mut(name),
            InodeKind::File(_) => None,
        }
    }

    pub fn list_children(&self) -> Vec<&String> {
        match &self.kind {
            InodeKind::Directory(children) => children.keys().collect(),
            InodeKind::File(_) => Vec::new(),
        }
    }
}
