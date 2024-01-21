#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(custom_test_frameworks)] // Because #![no_std]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]
use core::panic::PanicInfo;
use x86_64::registers::control::Cr3;
use yonti_os::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("Welcome to YontiOS{}", "!");
    yonti_os::init();
    let (level_4_page_table, _) = Cr3::read();
    println!(
        "Level  level 4 page table at: {:?}",
        level_4_page_table.start_address()
    );

    #[cfg(test)]
    test_main();

    println!("Didn't crash!");
    yonti_os::hlt_loop();
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
