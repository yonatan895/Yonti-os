# AGENTS.md — Yonti-os

Bare-metal x86_64 kernel in Rust. Follows Philipp Oppermann's "Writing an OS in Rust" series.

## Build requirements

```sh
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
cargo install bootimage
```

- **Nightly only** (pinned by `rust-toolchain.toml`). Required for `build-std`, `abi_x86_interrupt`, `custom_test_frameworks`.
- **QEMU**: `qemu-system-x86_64` must be installed to run (`cargo run`) or test (`cargo test`).
- `rust-src` and `llvm-tools-preview` are needed for `build-std` and linking.

## Build commands

```sh
cargo build              # compile kernel
cargo run                # build + run in QEMU (serial output on stdio)
cargo bootimage          # only build the bootable .bin (no QEMU)
```

## Testing

```sh
# Run all tests except flaky stack_overflow:
cargo test -- --skip stack_overflow

# Run a single test binary:
cargo test --test file_system
cargo test --test heap_allocation
```

**Critical test quirk:** Tests using `#![feature(custom_test_frameworks)]` and `#[reexport_test_harness_main]` must use cargo's **default harness** (`harness = true`). Setting `harness = false` in `[[test]]` prevents `test_main` generation from the compiler feature. Only `should_panic` and `stack_overflow` override this — they don't use `custom_test_frameworks`.

`stack_overflow` is a known-flaky test that triggers a real stack overflow. It may hang — skip it by default.

## Architecture

### Build pipeline
1. `cargo` compiles the kernel ELF via `build-std` for custom target `x86_64-yonti_os.json`
2. `bootimage` wraps the ELF + the `bootloader` crate into a bootable BIOS disk image
3. QEMU boots the image with `-display none -serial stdio` (configured in `Cargo.toml` `[package.metadata.bootimage]`)

### Custom target (`x86_64-yonti_os.json`)
- Based on `x86_64-unknown-none`, panic=abort, redzone disabled
- `target-pointer-width` and `target-c-int-width` must be **integers**, not string-quoted (Rust 1.83+)
- `.cargo/config.toml` must include `json-target-spec = true` under `[unstable]`

### Source layout
```
src/
  main.rs          # kernel entry (bootloader entry_point!), heap init, executor, demo_fs
  lib.rs           # crate root, init(), test framework, QEMU exit
  gdt.rs           # Global Descriptor Table + TSS (double fault IST)
  interrupts.rs    # IDT, PIC, keyboard/timer/page fault handlers
  memory.rs        # OffsetPageTable, BootInfoFrameAllocator
  allocator.rs     # Global allocator (FixedSizeBlockAllocator), heap init
  allocator/       # bump, fixed_size_block, linked_list allocators
  serial.rs        # UART 16550, serial_print! macros
  sse.rs           # SSE enablement via inline asm
  vga_buffer.rs    # VGA 80x25 text mode, println! macros
  task/
    mod.rs         # Task, TaskId
    executor.rs    # async executor (BTreeMap, ArrayQueue, HLT idle)
    keyboard.rs    # async scancode stream
  fs/
    mod.rs         # FileSystem (global FS lazy_static)
    inode.rs       # Inode, InodeKind (File | Directory)
tests/
  basic_boot.rs, heap_allocation.rs, file_system.rs, should_panic.rs, stack_overflow.rs
```

### Output routing
`println!` writes to **both** VGA buffer and serial port. `serial_println!` writes only to serial. The test framework uses serial output (QEMU `-serial stdio`).

### Filesystem (`src/fs/`)
- In-memory only, no disk. All data in `Vec<u8>` on the heap.
- Global `FS` lazy_static wraps a `spin::Mutex<FileSystem>`.
- Hierarchical: root `/`, create files/dirs, read/write/append/list.

## Dependency API gotchas

- **`x86_64` 0.15.4+**: `gdt.push()` is private → use `gdt.append()`. `idt.interrupts[index]` is private → use `idt[index]` (Index<u8> trait).
- **`bootloader` 0.9.x**: `entry_point!` macro, `BootInfo`, `BootInfoFrameAllocator` from `bootloader::bootinfo`.
- **`spin` 0.5.x**: `spin::Mutex`, not `std::sync::Mutex`.
- **`linked_list_allocator` 0.9.x**: `Heap::empty()` is the initializer.

## Runtime behavior

If the kernel hangs or triple-faults silently, check:
- Missing `#[global_allocator]` init — heap must be initialized before any allocation
- Page table mappings for heap region — `init_heap` maps pages at `0x4444_4444_0000`
- `-display none -serial stdio` in bootimage config (headless QEMU)
