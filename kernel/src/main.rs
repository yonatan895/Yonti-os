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
use yonti_os::fs;
use yonti_os::log;
use yonti_os::memory;
use yonti_os::println;
use yonti_os::task::keyboard;
use yonti_os::task::{executor::Executor, Task};

entry_point!(kernel_main, config = &yonti_os::BOOTLOADER_CONFIG);
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    yonti_os::init();

    if let Some(fb) = boot_info.framebuffer.take() {
        let info = fb.info();
        let buffer = fb.into_buffer();
        yonti_os::framebuffer::init(buffer, info);
    }

    log::init(log::LevelFilter::Info).expect("logger already set");
    log::info!("framebuffer initialized");

    let phys_mem_offset = VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("physical memory offset not set"),
    );

    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        memory::buddy::BuddyAllocator::new(&boot_info.memory_regions, phys_mem_offset.as_u64());

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("Heap init failed");
    log::info!("heap initialized");

    demo_fs();
    log::info!("filesystem demo done");

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
    fs.write_file("/home/test", b"nested file!")
        .expect("write /home/test");
    let contents = fs.list_dir("/").expect("list /");
    println!("[fs] / contents: {:?}", contents);
    let contents = fs.list_dir("/home").expect("list /home");
    println!("[fs] /home contents: {:?}", contents);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("PANIC: {}", info);
    yonti_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
