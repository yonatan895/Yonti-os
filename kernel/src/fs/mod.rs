/// In-memory filesystem providing hierarchical directories and files.
///
/// All data is stored in heap-allocated `Vec<u8>` — no disk driver needed.
/// Thread-safe via `spin::RwLock` around the global [`FS`] instance.
///
/// # Limits
///
/// | Resource        | Limit  |
/// |-----------------|--------|
/// | Max file size   | 64 KiB |
/// | Max children/dir| 128    |
/// | Max path depth  | 16     |
/// | Max total inodes| 512    |
///
/// # Example
///
/// ```ignore
/// use yonti_os::fs::FS;
///
/// let mut fs = FS.write();
/// fs.create_file("/hello.txt").unwrap();
/// fs.write_file("/hello.txt", b"Hello!").unwrap();
/// assert_eq!(fs.read_file("/hello.txt").unwrap(), b"Hello!");
/// ```
pub mod inode;

use alloc::string::String;
use alloc::vec::Vec;
use inode::{Inode, InodeKind};
use spin::RwLock;

const MAX_FILE_SIZE: usize = 65536;
const MAX_DIR_CHILDREN: usize = 128;
const MAX_PATH_DEPTH: usize = 16;
const MAX_TOTAL_INODES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AlreadyExists,
    NotAFile,
    NotADirectory,
    InvalidPath,
    FileTooLarge,
    TooManyChildren,
    PathTooDeep,
    TooManyInodes,
    DirectoryNotEmpty,
}

#[derive(Debug)]
pub struct FileSystem {
    root: Inode,
    inode_count: usize,
}

impl FileSystem {
    pub fn new() -> Self {
        FileSystem {
            root: Inode::new_directory("/"),
            inode_count: 1,
        }
    }

    pub fn create_file(&mut self, path: &str) -> Result<(), FsError> {
        if self.inode_count >= MAX_TOTAL_INODES {
            return Err(FsError::TooManyInodes);
        }
        let (parent, name) = resolve_path(path)?;
        let dir = find_dir_mut(&mut self.root, parent)?;
        if dir.find_child(name).is_some() {
            return Err(FsError::AlreadyExists);
        }
        if dir_child_count(dir) >= MAX_DIR_CHILDREN {
            return Err(FsError::TooManyChildren);
        }
        match &mut dir.kind {
            InodeKind::Directory(children) => {
                children.insert(String::from(name), Inode::new_file(name));
                self.inode_count += 1;
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    pub fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        if self.inode_count >= MAX_TOTAL_INODES {
            return Err(FsError::TooManyInodes);
        }
        let (parent, name) = resolve_path(path)?;
        let dir = find_dir_mut(&mut self.root, parent)?;
        if dir.find_child(name).is_some() {
            return Err(FsError::AlreadyExists);
        }
        if dir_child_count(dir) >= MAX_DIR_CHILDREN {
            return Err(FsError::TooManyChildren);
        }
        match &mut dir.kind {
            InodeKind::Directory(children) => {
                children.insert(String::from(name), Inode::new_directory(name));
                self.inode_count += 1;
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let inode = find_inode(&self.root, path)?;
        inode.read().map(|d| d.to_vec()).ok_or(FsError::NotAFile)
    }

    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsError> {
        if data.len() > MAX_FILE_SIZE {
            return Err(FsError::FileTooLarge);
        }
        let inode = find_inode_mut(&mut self.root, path)?;
        inode.write(data)
    }

    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<(), FsError> {
        let inode = find_inode_mut(&mut self.root, path)?;
        let current_len = match &inode.kind {
            InodeKind::File(buf) => buf.len(),
            InodeKind::Directory(_) => return Err(FsError::NotAFile),
        };
        if current_len.saturating_add(data.len()) > MAX_FILE_SIZE {
            return Err(FsError::FileTooLarge);
        }
        inode.append(data)
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, FsError> {
        let inode = find_inode(&self.root, path)?;
        match &inode.kind {
            InodeKind::Directory(children) => Ok(children.keys().cloned().collect()),
            InodeKind::File(_) => Err(FsError::NotADirectory),
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        find_inode(&self.root, path).is_ok()
    }

    pub fn remove(&mut self, path: &str) -> Result<(), FsError> {
        let (parent, name) = resolve_path(path)?;
        let dir = find_dir_mut(&mut self.root, parent)?;
        dir.remove_child(name)?;
        self.inode_count = self.inode_count.saturating_sub(1);
        Ok(())
    }
}

impl Default for FileSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn dir_child_count(dir: &Inode) -> usize {
    match &dir.kind {
        InodeKind::Directory(children) => children.len(),
        _ => 0,
    }
}

fn find_inode<'a>(root: &'a Inode, path: &str) -> Result<&'a Inode, FsError> {
    let parts = split_path(path);
    if parts.len() > MAX_PATH_DEPTH {
        return Err(FsError::PathTooDeep);
    }
    let mut current = root;
    for part in &parts {
        current = current.find_child(part).ok_or(FsError::NotFound)?;
    }
    Ok(current)
}

fn find_inode_mut<'a>(root: &'a mut Inode, path: &str) -> Result<&'a mut Inode, FsError> {
    let parts = split_path(path);
    if parts.len() > MAX_PATH_DEPTH {
        return Err(FsError::PathTooDeep);
    }
    let mut current = root;
    for part in &parts {
        current = current.find_child_mut(part).ok_or(FsError::NotFound)?;
    }
    Ok(current)
}

fn find_dir_mut<'a>(root: &'a mut Inode, path: &str) -> Result<&'a mut Inode, FsError> {
    let dir = find_inode_mut(root, path)?;
    match dir.kind {
        InodeKind::Directory(_) => Ok(dir),
        InodeKind::File(_) => Err(FsError::NotADirectory),
    }
}

fn resolve_path(path: &str) -> Result<(&str, &str), FsError> {
    if path.is_empty() || path == "/" {
        return Err(FsError::InvalidPath);
    }
    let path = path.trim_matches('/');
    match path.rfind('/') {
        Some(idx) => {
            let parent = &path[..idx];
            let name = &path[idx + 1..];
            if name.is_empty() || name == "." || name == ".." {
                return Err(FsError::InvalidPath);
            }
            Ok((parent, name))
        }
        None => {
            if path == "." || path == ".." {
                return Err(FsError::InvalidPath);
            }
            Ok(("", path))
        }
    }
}

fn split_path(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect()
}

use lazy_static::lazy_static;

lazy_static! {
    pub static ref FS: RwLock<FileSystem> = RwLock::new(FileSystem::new());
}
