use std::process::{self, Command};

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bios".to_string());

    let bios_img = env!("BIOS_IMG");
    let uefi_img = env!("UEFI_IMG");

    match mode.as_str() {
        "bios" => run_bios(bios_img),
        "uefi" => run_uefi(uefi_img),
        _ => {
            eprintln!("Usage: cargo run -- [bios|uefi]");
            process::exit(1);
        }
    }
}

fn run_bios(img: &str) {
    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.arg("-nographic");
    cmd.arg("-drive")
        .arg(format!("format=raw,file={img}"))
        .arg("-no-reboot")
        .arg("-device")
        .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
    let status = cmd.status().unwrap();
    process::exit(status.code().unwrap_or(1));
}

fn run_uefi(img: &str) {
    #[cfg(feature = "uefi")]
    {
        use ovmf_prebuilt::{Arch, FileType, Prebuilt, Source};

        let prebuilt = Prebuilt::fetch(Source::LATEST, ".").expect("failed to fetch OVMF");
        let ovmf = prebuilt.get_file(Arch::X64, FileType::Code);

        let mut cmd = Command::new("qemu-system-x86_64");
        cmd.arg("-nographic");
        cmd.arg("-bios").arg(ovmf);
        cmd.arg("-drive")
            .arg(format!("format=raw,file={img}"))
            .arg("-no-reboot")
            .arg("-device")
            .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
        let status = cmd.status().unwrap();
        process::exit(status.code().unwrap_or(1));
    }

    #[cfg(not(feature = "uefi"))]
    {
        let _ = img;
        eprintln!("error: UEFI support not compiled (build with --features uefi)");
        process::exit(1);
    }
}
