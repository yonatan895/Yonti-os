/// QEMU test runner — wraps a kernel ELF into a bootable BIOS image
/// and runs it in QEMU. Maps isa-debug-exit codes to exit status.
///
/// Usage: qemu_runner <kernel_elf>

use bootloader::DiskImageBuilder;
use std::path::PathBuf;
use std::process::{self, Command};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <kernel-elf>", args[0]);
        process::exit(2);
    }

    let kernel_path = PathBuf::from(&args[1]);
    let tmp_dir = PathBuf::from("/tmp/yonti-bazel-test");
    std::fs::create_dir_all(&tmp_dir).expect("create tmp dir");
    let bios_img = tmp_dir.join("test_bios.img");

    // Build bootable BIOS disk image from the kernel ELF
    DiskImageBuilder::new(&kernel_path)
        .create_bios_image(&bios_img)
        .expect("failed to create BIOS test image");

    // Run QEMU
    let status = Command::new("qemu-system-x86_64")
        .arg("-nographic")
        .arg("-drive")
        .arg(format!("format=raw,file={}", bios_img.display()))
        .arg("-no-reboot")
        .arg("-device")
        .arg("isa-debug-exit,iobase=0xf4,iosize=0x04")
        .status()
        .expect("failed to run QEMU");

    // Cleanup
    let _ = std::fs::remove_file(&bios_img);

    // isa-debug-exit: success=33, failure=35
    match status.code().unwrap_or(1) {
        33 => process::exit(0),
        35 => process::exit(1),
        c => process::exit(c),
    }
}
