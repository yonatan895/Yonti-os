#[test_case]
fn create_and_read_file() {
    let mut fs = yonti_os::fs::FS.write();
    fs.create_file("/test.txt").expect("create /test.txt");
    fs.write_file("/test.txt", b"Hello, world!")
        .expect("write /test.txt");
    let data = fs.read_file("/test.txt").expect("read /test.txt");
    assert_eq!(data, b"Hello, world!");
}

#[test_case]
fn create_and_list_directory() {
    let mut fs = yonti_os::fs::FS.write();
    fs.create_dir("/mydir").expect("create /mydir");
    fs.create_file("/mydir/a").expect("create /mydir/a");
    fs.create_file("/mydir/b").expect("create /mydir/b");
    let list = fs.list_dir("/mydir").expect("list /mydir");
    assert_eq!(list.len(), 2);
    assert!(list.contains(&"a"));
    assert!(list.contains(&"b"));
}

#[test_case]
fn append_to_file() {
    let mut fs = yonti_os::fs::FS.write();
    fs.create_file("/append.txt").expect("create /append.txt");
    fs.write_file("/append.txt", b"first").expect("write first");
    fs.append_file("/append.txt", b"second")
        .expect("append second");
    let data = fs.read_file("/append.txt").expect("read /append.txt");
    assert_eq!(data, b"firstsecond");
}

#[test_case]
fn file_exists_and_nonexistent() {
    let mut fs = yonti_os::fs::FS.write();
    fs.create_file("/real.txt").expect("create /real.txt");
    assert!(fs.exists("/real.txt"));
    assert!(!fs.exists("/nope.txt"));
}

#[test_case]
fn nested_paths() {
    let mut fs = yonti_os::fs::FS.write();
    fs.create_dir("/a").expect("create /a");
    fs.create_dir("/a/b").expect("create /a/b");
    fs.create_file("/a/b/c.txt").expect("create /a/b/c.txt");
    fs.write_file("/a/b/c.txt", b"deep").expect("write deep");
    assert_eq!(fs.read_file("/a/b/c.txt").unwrap(), b"deep");
}

#[test_case]
fn delete_file_and_directory() {
    let mut fs = yonti_os::fs::FS.write();
    fs.create_file("/todelete.txt")
        .expect("create /todelete.txt");
    assert!(fs.exists("/todelete.txt"));
    fs.remove("/todelete.txt").expect("remove /todelete.txt");
    assert!(!fs.exists("/todelete.txt"));

    fs.create_dir("/deldir").expect("create /deldir");
    fs.create_file("/deldir/file").expect("create /deldir/file");
    assert!(fs.exists("/deldir/file"));
    fs.remove("/deldir/file").expect("remove /deldir/file");
    assert!(!fs.exists("/deldir/file"));
    fs.remove("/deldir").expect("remove /deldir");
    assert!(!fs.exists("/deldir"));
}

#[test_case]
fn invalid_dot_paths() {
    let mut fs = yonti_os::fs::FS.write();
    assert!(fs.create_file("/.").is_err());
    assert!(fs.create_file("/..").is_err());
    assert!(fs.create_dir("/.").is_err());
    assert!(fs.create_dir("/..").is_err());
    assert!(fs.create_file("/a/.").is_err());
    assert!(fs.create_file("/a/..").is_err());
}
