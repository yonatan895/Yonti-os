#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use yonti_os::allocator;
use yonti_os::framebuffer;
use yonti_os::memory::{self, buddy::BuddyAllocator};

entry_point!(main, config = &yonti_os::BOOTLOADER_CONFIG);

fn main(boot_info: &'static mut BootInfo) -> ! {
    yonti_os::init();
    if let Some(fb) = boot_info.framebuffer.take() {
        let info = fb.info();
        let buffer = fb.into_buffer();
        framebuffer::init(buffer, info);
    }

    let phys_mem_offset = VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("physical_memory_offset not set"),
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        BuddyAllocator::new(&boot_info.memory_regions, phys_mem_offset.as_u64());
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    test_main();
    yonti_os::hlt_loop();
}

#[test_case]
fn create_and_read_file() {
    let mut fs = yonti_os::fs::FS.lock();
    fs.create_file("/test.txt").expect("create /test.txt");
    fs.write_file("/test.txt", b"Hello, world!")
        .expect("write /test.txt");
    let data = fs.read_file("/test.txt").expect("read /test.txt");
    assert_eq!(data, b"Hello, world!");
}

#[test_case]
fn create_and_list_directory() {
    let mut fs = yonti_os::fs::FS.lock();
    fs.create_dir("/mydir").expect("create /mydir");
    fs.create_file("/mydir/a").expect("create /mydir/a");
    fs.create_file("/mydir/b").expect("create /mydir/b");
    let list = fs.list_dir("/mydir").expect("list /mydir");
    assert_eq!(list.len(), 2);
    assert!(list.contains(&alloc::string::String::from("a")));
    assert!(list.contains(&alloc::string::String::from("b")));
}

#[test_case]
fn append_to_file() {
    let mut fs = yonti_os::fs::FS.lock();
    fs.create_file("/append.txt").expect("create /append.txt");
    fs.write_file("/append.txt", b"first").expect("write first");
    fs.append_file("/append.txt", b"second")
        .expect("append second");
    let data = fs.read_file("/append.txt").expect("read /append.txt");
    assert_eq!(data, b"firstsecond");
}

#[test_case]
fn file_exists_and_nonexistent() {
    let mut fs = yonti_os::fs::FS.lock();
    fs.create_file("/real.txt").expect("create /real.txt");
    assert!(fs.exists("/real.txt"));
    assert!(!fs.exists("/nope.txt"));
}

#[test_case]
fn nested_paths() {
    let mut fs = yonti_os::fs::FS.lock();
    fs.create_dir("/a").expect("create /a");
    fs.create_dir("/a/b").expect("create /a/b");
    fs.create_file("/a/b/c.txt").expect("create /a/b/c.txt");
    fs.write_file("/a/b/c.txt", b"deep").expect("write deep");
    assert_eq!(fs.read_file("/a/b/c.txt").unwrap(), b"deep");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
