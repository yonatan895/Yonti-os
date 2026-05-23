#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(abi_x86_interrupt)]
#![allow(unexpected_cfgs)]
extern crate alloc;

pub mod allocator;
pub mod array_queue;
pub mod async_utils;
pub mod font;
pub mod framebuffer;
pub mod fs;
pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod once_cell;
pub mod pic;
pub mod serial;
pub mod sse;
pub mod task;
pub mod uart;
pub mod vga_buffer;

use core::panic::PanicInfo;

use bootloader_api::config::{BootloaderConfig, Mapping};

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.kernel_stack_size = 20 * 4096;
    config
};

pub fn init() {
    use x86_64::instructions;
    // Bootloader 0.11 already sets up GDT/TSS.
    // Only SSE, IDT, and PIC need explicit init.
    unsafe {
        sse::init();
    }
    interrupts::init_idt();
    unsafe { interrupts::PICS.lock().initialize() };
    instructions::interrupts::enable();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    unsafe {
        let mut port = x86_64::instructions::port::Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}
pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

#[cfg(all(test, not(bazel)))]
use bootloader_api::{entry_point, BootInfo};

#[cfg(all(test, not(bazel)))]
entry_point!(test_kernel_main, config = &BOOTLOADER_CONFIG);

#[cfg(all(test, not(bazel)))]
fn test_kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init();
    if let Some(fb) = boot_info.framebuffer.take() {
        let info = fb.info();
        let buffer = fb.into_buffer();
        framebuffer::init(buffer, info);
    }
    test_main();
    hlt_loop();
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(all(test, not(bazel)))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}
