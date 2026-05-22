use bootloader::DiskImageBuilder;
use std::process::{self, Command};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: test-runner <kernel-binary>");
        process::exit(1);
    }
    let kernel_path = PathBuf::from(&args[1]);

    // Create a temporary directory for the disk image
    let tmp_dir = PathBuf::from("/tmp/yonti-os-test-runner");
    std::fs::create_dir_all(&tmp_dir).expect("failed to create tmp dir");

    let bios_img = tmp_dir.join("test_bios.img");

    // Build bootable BIOS disk image from the test kernel binary
    let builder = DiskImageBuilder::new(kernel_path);
    builder.create_bios_image(&bios_img).expect("failed to create BIOS test image");

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

    // isa-debug-exit maps: exit_code = (val << 1) | 1
    // QEMU exits with: (val << 1) | 1
    // Our kernel uses: Success = 0x10, Failed = 0x11
    // So QEMU exit code for Success = (0x10 << 1) | 1 = 33
    // QEMU exit code for Failed  = (0x11 << 1) | 1 = 35
    let code = status.code().unwrap_or(1);
    match code {
        33 => process::exit(0),   // Success
        35 => process::exit(1),   // Failed
        _ => process::exit(code),
    }
}