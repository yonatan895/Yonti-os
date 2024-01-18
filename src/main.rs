#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

#![feature(custom_test_frameworks)] // Because #![no_std]
#![test_runner(yonti_os::test_runner)]
#![reexport_test_harness_main = "test_main"]
use core::panic::PanicInfo;
use yonti_os::println;



#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("Welcome to YontiOS{}", "!");

    #[cfg(test)]
    test_main();

    loop {}
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    yonti_os::test_panic_handler(info)
}
