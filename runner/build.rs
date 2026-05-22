use bootloader::DiskImageBuilder;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Build the kernel for the target (must be explicit — runner config overrides)
    let kernel_dir = PathBuf::from("../kernel");
    let status = Command::new("cargo")
        .current_dir(&kernel_dir)
        .args(["build", "--target", "x86_64-unknown-none"])
        .status()
        .expect("failed to build kernel");
    assert!(status.success(), "kernel build failed");

    // Find the kernel binary (workspace outputs to root target dir)
    let target_dir = kernel_dir
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/x86_64-unknown-none/debug/yonti_os");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    let bios_img = out_dir.join("bios.img");
    DiskImageBuilder::new(target_dir.clone())
        .create_bios_image(&bios_img)
        .unwrap();

    let uefi_img = out_dir.join("uefi.img");
    DiskImageBuilder::new(target_dir)
        .create_uefi_image(&uefi_img)
        .unwrap();

    println!("cargo:rustc-env=BIOS_IMG={}", bios_img.display());
    println!("cargo:rustc-env=UEFI_IMG={}", uefi_img.display());
}
