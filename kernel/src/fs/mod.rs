/// In-memory filesystem providing hierarchical directories and files.
///
/// All data is stored in heap-allocated `Vec<u8>` — no disk driver needed.
/// Thread-safe via `spin::Mutex` around the global [`FS`] instance.
///
/// # Example
///
/// ```ignore
/// use yonti_os::fs::FS;
///
/// let mut fs = FS.lock();
/// fs.create_file("/hello.txt").unwrap();
/// fs.write_file("/hello.txt", b"Hello!").unwrap();
/// assert_eq!(fs.read_file("/hello.txt").unwrap(), b"Hello!");
/// ```
pub mod inode;

use alloc::string::String;
use alloc::vec::Vec;
use inode::{Inode, InodeKind};
use spin::Mutex;

pub struct FileSystem {
    root: Inode,
}

impl FileSystem {
    pub fn new() -> Self {
        FileSystem {
            root: Inode::new_directory("/"),
        }
    }

    pub fn create_file(&mut self, path: &str) -> Result<(), &'static str> {
        let (parent, name) = resolve_path(path)?;
        let dir = find_dir_mut(&mut self.root, parent)?;
        if dir.find_child(name).is_some() {
            return Err("file already exists");
        }
        match &mut dir.kind {
            InodeKind::Directory(children) => {
                children.insert(String::from(name), Inode::new_file(name));
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    pub fn create_dir(&mut self, path: &str) -> Result<(), &'static str> {
        let (parent, name) = resolve_path(path)?;
        let dir = find_dir_mut(&mut self.root, parent)?;
        if dir.find_child(name).is_some() {
            return Err("directory already exists");
        }
        match &mut dir.kind {
            InodeKind::Directory(children) => {
                children.insert(String::from(name), Inode::new_directory(name));
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, &'static str> {
        let inode = find_inode(&self.root, path)?;
        inode.read().map(|d| d.to_vec()).ok_or("not a file")
    }

    /// Write data to a file, replacing any existing contents.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), &'static str> {
        let inode = find_inode_mut(&mut self.root, path)?;
        inode.write(data)
    }

    /// Append data to the end of a file.
    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<(), &'static str> {
        let inode = find_inode_mut(&mut self.root, path)?;
        inode.append(data)
    }

    /// List the names of all children in a directory.
    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, &'static str> {
        let inode = find_inode(&self.root, path)?;
        match &inode.kind {
            InodeKind::Directory(children) => Ok(children.keys().cloned().collect()),
            InodeKind::File(_) => Err("not a directory"),
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        find_inode(&self.root, path).is_ok()
    }
}

impl Default for FileSystem {
    fn default() -> Self {
        Self::new()
    }
}

fn find_inode<'a>(root: &'a Inode, path: &str) -> Result<&'a Inode, &'static str> {
    let parts = split_path(path);
    let mut current = root;
    for part in &parts {
        current = current.find_child(part).ok_or("path not found")?;
    }
    Ok(current)
}

fn find_inode_mut<'a>(root: &'a mut Inode, path: &str) -> Result<&'a mut Inode, &'static str> {
    let parts = split_path(path);
    let mut current = root;
    for part in &parts {
        current = current.find_child_mut(part).ok_or("path not found")?;
    }
    Ok(current)
}

fn find_dir_mut<'a>(root: &'a mut Inode, path: &str) -> Result<&'a mut Inode, &'static str> {
    let dir = find_inode_mut(root, path)?;
    match dir.kind {
        InodeKind::Directory(_) => Ok(dir),
        InodeKind::File(_) => Err("not a directory"),
    }
}

fn resolve_path(path: &str) -> Result<(&str, &str), &'static str> {
    if path.is_empty() || path == "/" {
        return Err("invalid path");
    }
    let path = path.trim_matches('/');
    match path.rfind('/') {
        Some(idx) => {
            let parent = &path[..idx];
            let name = &path[idx + 1..];
            if name.is_empty() {
                return Err("invalid path");
            }
            Ok((parent, name))
        }
        None => Ok(("", path)),
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
    pub static ref FS: Mutex<FileSystem> = Mutex::new(FileSystem::new());
}
