#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use yonti_os::allocator::HEAP_SIZE;
use yonti_os::framebuffer;
use yonti_os::memory::{self, BootInfoFrameAllocator};

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
        unsafe { BootInfoFrameAllocator::init(&mut boot_info.memory_regions) };
    yonti_os::allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");

    test_main();
    yonti_os::hlt_loop();
}

#[test_case]
fn simple_allocation() {
    let heap_value_1 = Box::new(50);
    assert_eq!(*heap_value_1, 50);
}

#[test_case]
fn large_vec() {
    let n = 10000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2);
}

#[test_case]
fn many_boxes() {
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

#[test_case]
fn many_boxes_long_lived() {
    let long_lived = Box::new(1);
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    assert_eq!(*long_lived, 1);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
