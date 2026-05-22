# Yonti-os

A bare-metal x86_64 operating system kernel written in Rust.

## Requirements

- Rust nightly (enforced by `rust-toolchain.toml`)
- `rust-src` component
- `llvm-tools-preview` component
- QEMU (`qemu-system-x86`)
- Linux x86_64

## Setup & Build

```sh
git clone https://github.com/yonatan895/Yonti-os
cd Yonti-os

# Install nightly toolchain and required components
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly

# Install bootimage (builds bootable disk images)
cargo install bootimage

# Build the kernel
cargo build
```

## Run

```sh
cargo run
```

This boots the kernel in QEMU with serial output on stdio. The kernel initializes hardware, sets up memory, spawns async tasks, and demos the in-memory filesystem.

## Test

```sh
cargo test -- --skip stack_overflow
```

Tests run inside QEMU with serial output. The `stack_overflow` test is excluded because it triggers a real stack overflow and may hang depending on QEMU version.

## Features

- **VGA text mode** output with 16-color support
- **Serial port** (UART 16550) output for logging
- **GDT** with kernel code segment and TSS (double fault IST)
- **IDT** with handlers for breakpoint, double fault, page fault, timer, and keyboard
- **PIC** remapping (offsets 32/40)
- **SSE** enablement via CR0/CR4 registers
- **Paging** with `OffsetPageTable` from bootloader-provided mappings
- **Frame allocation** from bootloader memory map
- **Heap** (1 MiB at `0x4444_4444_0000`) with fixed-size block allocator and linked-list fallback
- **Async executor** with cooperative multitasking (`futures-util` streams)
- **Async keyboard** input via scancode queue and atomic waker

### In-Memory Filesystem (`src/fs/`)

A simple, safe, in-memory filesystem — no disk driver required. All data lives in heap-allocated `Vec<u8>`.

- Hierarchical directories with `BTreeMap` children
- Create, read, write, append files
- Create directories, list contents, check existence
- Nested path resolution (e.g. `/home/user/file.txt`)
- Thread-safe via `spin::Mutex`

```rust
use yonti_os::fs::FS;

let mut fs = FS.lock();
fs.create_file("/hello.txt").unwrap();
fs.write_file("/hello.txt", b"Hello, world!").unwrap();
assert_eq!(fs.read_file("/hello.txt").unwrap(), b"Hello, world!");
```
