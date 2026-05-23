use std::process::{self, Command};

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bios".to_string());

    let bios_img = env!("BIOS_IMG");
    let uefi_img = env!("UEFI_IMG");

    let no_graphic = !std::env::args().any(|a| a == "--display");

    match mode.as_str() {
        "bios" => run_bios(bios_img, no_graphic),
        "uefi" => run_uefi(uefi_img, no_graphic),
        "gui" => run_bios(bios_img, false),
        _ => {
            eprintln!("Usage: cargo run -- [bios|uefi|gui] [--display]");
            process::exit(1);
        }
    }
}

fn qemu_base_args(img: &str, no_graphic: bool) -> Command {
    let mut cmd = Command::new("qemu-system-x86_64");
    if no_graphic {
        cmd.arg("-nographic");
    }
    cmd.arg("-drive")
        .arg(format!("format=raw,file={img}"))
        .arg("-no-reboot")
        .arg("-device")
        .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
    cmd
}

fn run_bios(img: &str, no_graphic: bool) {
    let status = qemu_base_args(img, no_graphic).status().unwrap();
    process::exit(status.code().unwrap_or(1));
}

fn run_uefi(img: &str, no_graphic: bool) {
    #[cfg(feature = "uefi")]
    {
        use ovmf_prebuilt::{Arch, FileType, Prebuilt, Source};

        let prebuilt =
            Prebuilt::fetch(Source::LATEST, "/tmp/yonti-os-ovmf").expect("failed to fetch OVMF");
        let ovmf = prebuilt.get_file(Arch::X64, FileType::Code);

        let mut cmd = qemu_base_args(img, no_graphic);
        cmd.arg("-bios").arg(ovmf);
        let status = cmd.status().unwrap();
        process::exit(status.code().unwrap_or(1));
    }

    #[cfg(not(feature = "uefi"))]
    {
        let _ = (img, no_graphic);
        eprintln!("error: UEFI support not compiled (build with --features uefi)");
        process::exit(1);
    }
}
