#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[path = "common/basic_boot.rs"]
mod basic_boot;
#[path = "common/file_system.rs"]
mod file_system;
#[path = "common/heap_allocation.rs"]
mod heap_allocation;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use yonti_os::allocator;
use yonti_os::framebuffer;
use yonti_os::memory::{self, buddy::BuddyAllocator};

entry_point!(test_kernel_main, config = &yonti_os::BOOTLOADER_CONFIG);

fn test_kernel_main(boot_info: &'static mut BootInfo) -> ! {
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
            .expect("physical memory offset not set"),
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        BuddyAllocator::new(&boot_info.memory_regions, phys_mem_offset.as_u64());
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    test_main();
    yonti_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
