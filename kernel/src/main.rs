#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use x86_64::instructions::interrupts;
use yonti_os::allocator;
use yonti_os::apic;
use yonti_os::fs;
use yonti_os::interrupts as interrupts_mod;
use yonti_os::log;
use yonti_os::memory;
use yonti_os::println;
use yonti_os::shell;
use yonti_os::task::{Task, executor::Executor};
use yonti_os::trace;

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
    trace::init();

    unsafe {
        init_apic(boot_info, phys_mem_offset);
    }

    demo_fs();
    log::info!("filesystem demo done");

    #[cfg(test)]
    test_main();

    let mut executor = Executor::new();
    executor.spawn(Task::new(example_task()));
    executor.spawn(Task::new(shell::shell_task()));
    executor.run();
}

async fn async_number() -> u32 {
    5234
}

async fn example_task() {
    let number = async_number().await;
    println!("async number: {}", number);
}

unsafe fn init_apic(boot_info: &BootInfo, phys_offset: VirtAddr) {
    let rsdp_addr = match boot_info.rsdp_addr.into_option() {
        Some(addr) => addr,
        None => {
            log::warn!("ACPI: no RSDP found — using PIC fallback");
            interrupts::enable();
            return;
        }
    };

    let info = match unsafe { apic::detect(rsdp_addr, phys_offset.as_u64()) } {
        Some(info) => info,
        None => {
            log::warn!("APIC: detection failed — using PIC fallback");
            interrupts::enable();
            return;
        }
    };

    interrupts::disable();

    let ok = unsafe { apic::init(&info, phys_offset.as_u64()) };

    if ok {
        unsafe {
            interrupts_mod::PICS.lock().mask_all();
        }
        log::info!("APIC: PIC masked, APIC routing active");
    } else {
        log::warn!("APIC: init failed — using PIC fallback");
    }

    interrupts::enable();
}

fn demo_fs() {
    let mut fs = fs::FS.write();
    fs.create_file("/hello.txt").expect("create /hello.txt");
    fs.write_file("/hello.txt", b"Hello from Yonti-os filesystem!")
        .expect("write /hello.txt");
    if let Ok(data) = fs.read_file("/hello.txt")
        && let Ok(s) = core::str::from_utf8(&data)
    {
        println!("[fs] /hello.txt: {}", s);
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
    yonti_os::debug::crash_dump(
        &alloc::format!("{}", info),
        info.location().map_or("?", |l| l.file()),
        info.location().map_or(0, |l| l.line()),
    );
    yonti_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
