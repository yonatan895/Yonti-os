#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(custom_test_frameworks)] // Because #![no_std]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use yonti_os::allocator;
use yonti_os::fs;
use yonti_os::memory;
use yonti_os::println;
use yonti_os::task::keyboard;
use yonti_os::task::{executor::Executor, Task};

entry_point!(kernel_main);
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    println!("Welcome to YontiOS{}", "!");
    yonti_os::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);

    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("Heap init failed");

    demo_fs();

    #[cfg(test)]
    test_main();

    let mut executor = Executor::new();
    executor.spawn(Task::new(example_task()));
    executor.spawn(Task::new(keyboard::print_keypresses()));
    executor.run();
}

async fn async_number() -> u32 {
    5234
}

async fn example_task() {
    let number = async_number().await;
    println!("async number: {}", number);
}

fn demo_fs() {
    let mut fs = fs::FS.lock();
    fs.create_file("/hello.txt").expect("create /hello.txt");
    fs.write_file("/hello.txt", b"Hello from Yonti-os filesystem!")
        .expect("write /hello.txt");
    if let Ok(data) = fs.read_file("/hello.txt") {
        if let Ok(s) = core::str::from_utf8(&data) {
            println!("[fs] /hello.txt: {}", s);
        }
    }
    fs.create_dir("/home").expect("create /home");
    fs.create_file("/home/test").expect("create /home/test");
    fs.write_file("/home/test", b"nested file!").expect("write /home/test");
    let contents = fs.list_dir("/").expect("list /");
    println!("[fs] / contents: {:?}", contents);
    let contents = fs.list_dir("/home").expect("list /home");
    println!("[fs] /home contents: {:?}", contents);
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    yonti_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
