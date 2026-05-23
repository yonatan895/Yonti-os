# AGENTS.md — Yonti-os

Bare-metal x86_64 kernel in Rust. Follows Philipp Oppermann's "Writing an OS in Rust" series.

## Build requirements

```sh
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
```

- **Nightly only** (pinned by `rust-toolchain.toml`). Required for `build-std`, `abi_x86_interrupt`, `custom_test_frameworks`.
- **QEMU**: `qemu-system-x86_64` must be installed to run or test (`sudo apt install qemu-system-x86`).
- `rust-src` and `llvm-tools-preview` are needed for `build-std` and linking.

## Build commands

```sh
# From the workspace root:
cd kernel && cargo build           # compile kernel (auto-targets x86_64-unknown-none)
cd runner && cargo build           # build runner + test-runner (host target)

# Run kernel in QEMU:
cd runner && cargo run --bin runner -- bios    # BIOS mode

# Run via convenience script:
./run_tests.sh                     # build + run all tests
./run_tests.sh basic_boot          # run a single test
```

## Testing

```sh
# Run all tests except flaky stack_overflow:
./run_tests.sh

# Run a single test:
./run_tests.sh basic_boot
./run_tests.sh file_system
./run_tests.sh heap_allocation

# Run with custom per-test timeout:
TIMEOUT=60 ./run_tests.sh
```

`run_tests.sh` builds the kernel, builds the `test-runner` binary, then for each test:

1. Compiles the test as a standalone kernel (`cargo build --test <name> --target x86_64-unknown-none`)
2. Uses `test-runner` to wrap the ELF into a bootable BIOS disk image
3. Runs it in QEMU with `isa-debug-exit` (exit code 33 = pass, 35 = fail)

**Critical test quirk:** Tests using `#![feature(custom_test_frameworks)]` and `#[reexport_test_harness_main]` must use cargo's **default harness** (`harness = true`). Setting `harness = false` in `[[test]]` prevents `test_main` generation from the compiler feature. Only `should_panic` and `stack_overflow` override this — they don't use `custom_test_frameworks`.

`stack_overflow` is a known-flaky test that triggers a real stack overflow. It may hang — skipped by default.

## CI/CD

All pushes to `master` and PRs targeting `master` are gated by `.github/workflows/ci.yml`:

| Job | What it checks |
|-----|---------------|
| `fmt` | `cargo fmt --all -- --check` (kernel workspace + runner workspace) |
| `clippy` | `cargo clippy -- -D warnings` for both kernel and runner |
| `deny` | `cargo deny check` (advisories, licenses, bans) for both workspaces |
| `build-and-test` | Full build + `./run_tests.sh` (all tests except `stack_overflow`) |

All four checks must pass before merging. The pipeline uses aggressive caching
(registry + target dirs, keyed by Cargo.lock hashes).

### Local pre-push checklist

```sh
# 1. Format
cargo fmt --all --
(cd runner && cargo fmt --)

# 2. Lint
(cd kernel && cargo clippy --target x86_64-unknown-none -- -D warnings)
(cd runner && cargo clippy -- -D warnings)

# 3. Test
./run_tests.sh

# 4. Security audit (optional, CI will catch failures)
cargo deny check
(cd runner && cargo deny check)
```

## Branching and PR workflow

Always work on a new branch off `master`. Never push directly to `master`,
reuse old branches whose PRs were already merged, or force-push.

```sh
# Starting new work
git checkout master
git pull origin master
git checkout -b feature/<description>    # or fix/<description>

# Make changes, verify locally
cargo fmt --all -- && (cd runner && cargo fmt --)
./run_tests.sh

# Push and create PR
git add -A
git commit -m "type: description"
git push origin feature/<description>
gh pr create --base master --title "..." --body "..."
```

Branch naming conventions:
- `feature/<description>` — new functionality
- `fix/<description>` — bug fixes
- `refactor/<description>` — code restructuring
- `ci/<description>` — CI/CD changes
- `docs/<description>` — documentation

## Architecture

### Build pipeline

1. `cargo` compiles the kernel ELF for `x86_64-unknown-none` via `build-std` (configured in `kernel/.cargo/config.toml`)
2. The `runner` crate's `build.rs` wraps the ELF into bootable BIOS and UEFI disk images via `DiskImageBuilder`
3. QEMU boots the image with `-nographic -device isa-debug-exit,iobase=0xf4,iosize=0x04`

### Workspace structure

```text
Yonti-os/                      # workspace root
├── Cargo.toml                 # workspace: members = ["kernel"]
├── .cargo/config.toml         # [unstable] bindeps = true
├── kernel/                    # workspace member (bare-metal kernel)
│   ├── Cargo.toml             # bootloader_api 0.11, all kernel deps
│   ├── .cargo/config.toml     # build-std, target = "x86_64-unknown-none"
│   ├── src/
│   │   ├── main.rs            # kernel_main entry, framebuffer init, executor
│   │   ├── lib.rs             # crate root, init(), test framework, QEMU exit
│   │   ├── framebuffer.rs     # FrameBufferWriter (pixel-based text renderer)
│   │   ├── font.rs            # 8×16 bitmap font (96 glyphs, 1536 bytes)
│   │   ├── vga_buffer.rs      # println! macros (routes to serial + framebuffer)
│   │   ├── serial.rs          # UART 16550, serial_print! macros
│   │   ├── gdt.rs             # GDT/TSS (not used — bootloader provides them)
│   │   ├── interrupts.rs      # IDT, PIC, keyboard/timer/page fault handlers
│   │   ├── memory.rs          # OffsetPageTable, BootInfoFrameAllocator
│   │   ├── allocator.rs       # Global allocator (FixedSizeBlockAllocator)
│   │   ├── allocator/         # bump, fixed_size_block, linked_list allocators
│   │   ├── sse.rs             # SSE enablement via inline asm
│   │   ├── task/
│   │   │   ├── mod.rs         # Task, TaskId
│   │   │   ├── executor.rs    # async executor (BTreeMap, ArrayQueue, HLT idle)
│   │   │   └── keyboard.rs    # async scancode stream
│   │   └── fs/
│   │       ├── mod.rs         # FileSystem (global FS lazy_static)
│   │       └── inode.rs       # Inode, InodeKind (File | Directory)
│   └── tests/
│       ├── basic_boot.rs, heap_allocation.rs, file_system.rs
│       ├── should_panic.rs, stack_overflow.rs
├── runner/                    # standalone workspace (host target)
│   ├── Cargo.toml             # separate [workspace], bootloader dep
│   ├── .cargo/config.toml     # target = "x86_64-unknown-linux-gnu"
│   ├── build.rs               # builds kernel, creates BIOS/UEFI disk images
│   └── src/
│       ├── main.rs            # QEMU launcher (bios/uefi modes)
│       └── test_runner.rs     # wraps test ELF → bootable image → QEMU
├── deny.toml                  # cargo-deny configuration
└── run_tests.sh               # convenience test script
```

### Output routing

`println!` writes to **both** framebuffer and serial port. `serial_println!` writes only to serial. The test framework uses serial output (QEMU `-serial stdio`).

### Framebuffer

- `FrameBufferWriter` stored in `Mutex<Option<FrameBufferWriter>>` global, initialized at runtime from `boot_info.framebuffer.take()`
- Supports RGB/BGR pixel formats, 8×16 font, automatic scrolling on newline
- Before init, `framebuffer::_print` is a no-op

### Filesystem (`kernel/src/fs/`)

- In-memory only, no disk. All data in `Vec<u8>` on the heap.
- Global `FS` lazy_static wraps a `spin::Mutex<FileSystem>`.
- Hierarchical: root `/`, create files/dirs, read/write/append/list.

## Dependency API gotchas

- **`bootloader_api` 0.11.x**: `entry_point!` macro with `config = &BOOTLOADER_CONFIG`. `BootInfo` has `memory_regions`, `physical_memory_offset.into_option()`, `framebuffer.take()`.
- **`x86_64` 0.15.4+**: `gdt.push()` is private → use `gdt.append()`. `idt.interrupts[index]` is private → use `idt[index]` (Index<u8> trait).
- **`spin` 0.5.x**: `spin::Mutex`, not `std::sync::Mutex`.
- **`linked_list_allocator` 0.9.x**: `Heap::empty()` is the initializer.

## Runtime behavior

If the kernel hangs or triple-faults silently, check:

- Missing `#[global_allocator]` init — heap must be initialized before any allocation
- Page table mappings for heap region — `init_heap` maps pages at `0x4444_4444_0000`
- Framebuffer init before first `println!` — first `println!` triggers `framebuffer::_print`, but before init it's a no-op; no crash but no output

## Commit conventions

```text
type: description
```

Where type is: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `chore`.

Branch naming: `feature/<description>` or `fix/<description>`.
