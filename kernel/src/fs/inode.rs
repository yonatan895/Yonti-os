use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// A file containing raw byte data, or a directory containing child inodes.
#[derive(Debug, Clone)]
pub enum InodeKind {
    File(Vec<u8>),
    Directory(BTreeMap<&'static str, Inode>),
}

/// A node in the filesystem tree, either a file or a directory.
#[derive(Debug, Clone)]
pub struct Inode {
    pub name: &'static str,
    pub kind: InodeKind,
}

impl Inode {
    /// Create a new empty file inode.
    pub const fn new_file(name: &'static str) -> Self {
        Inode {
            name,
            kind: InodeKind::File(Vec::new()),
        }
    }

    /// Create a new empty directory inode.
    pub const fn new_directory(name: &'static str) -> Self {
        Inode {
            name,
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
            InodeKind::File(buf) => {
                *buf = Vec::from(data);
                Ok(())
            }
            InodeKind::Directory(_) => Err("cannot write data to a directory"),
        }
    }

    pub fn append(&mut self, data: &[u8]) -> Result<(), &'static str> {
        match &mut self.kind {
            InodeKind::File(buf) => {
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

    pub fn list_children(&self) -> Vec<&'static str> {
        match &self.kind {
            InodeKind::Directory(children) => children.keys().copied().collect(),
            InodeKind::File(_) => Vec::new(),
        }
    }

    pub fn remove_child(&mut self, name: &str) -> Result<Inode, &'static str> {
        match &mut self.kind {
            InodeKind::Directory(children) => {
                children.remove(name).ok_or("file/directory not found")
            }
            InodeKind::File(_) => Err("cannot remove child from a file"),
        }
    }
}
