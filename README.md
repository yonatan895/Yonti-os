# Yonti-os

A bare-metal x86_64 operating system kernel written in Rust.

## Requirements

- **Rust nightly** (pinned by `rust-toolchain.toml`)
- `rust-src` and `llvm-tools-preview` components
- **QEMU** (`qemu-system-x86_64`)

Optional for local development:

- **cargo-deny** — license/security audit

## Setup

```sh
git clone https://github.com/yonatan895/Yonti-os
cd Yonti-os

rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
sudo apt install qemu-system-x86
```

## Build & Run

```sh
# Build kernel ELF
cd kernel && cargo build --target x86_64-unknown-none

# Run in QEMU (BIOS mode)
cd runner && cargo run --bin runner -- bios


```

## Test

```sh
# Run all tests (11 tests, 2 QEMU boots)
./run_tests.sh

# Run a single test binary
./run_tests.sh all
./run_tests.sh should_panic
```



## Features

- **Framebuffer** text renderer (8x16 VGA font, pixel-based)
- **Serial port** (UART 16550) output for logging and tests
- **IDT** with handlers for breakpoint, double fault, page fault, timer, and keyboard
- **PIC** remapping (offsets 32/40), inlined driver (`src/pic.rs`)
- **SSE** enablement via CR0/CR4 registers
- **Paging** with `OffsetPageTable` from bootloader-provided identity mapping
- **Buddy allocator** for physical frames (4 KiB–4 MiB blocks, O(log n))
- **TLSF heap allocator** (O(1) worst-case, 1 MiB at `0x4444_4444_0000`)
- **Async executor** with cooperative multitasking
- **Async keyboard** input via scancode queue and atomic waker
- **Observability**: structured logging (log crate), atomic metrics (`monitor.rs`), execution tracing ring buffer (`trace.rs`), crash diagnostics with register dump and stack backtrace (`debug.rs`)

### In-Memory Filesystem (`src/fs/`)

Hierarchical, heap-backed filesystem — no disk driver required.

- Create, read, write, append files
- Create directories, list contents, check existence
- Nested path resolution (`/home/user/file.txt`)
- Thread-safe via `spin::RwLock`

```rust
use yonti_os::fs::FS;

let mut fs = FS.write();
fs.create_file("/hello.txt").unwrap();
fs.write_file("/hello.txt", b"Hello, world!").unwrap();
assert_eq!(fs.read_file("/hello.txt").unwrap(), b"Hello, world!");
```

## CI Pipeline

PRs to `master` are gated by four checks:

| Job | What it checks |
|-----|---------------|
| `fmt` | `cargo fmt --check` for kernel + runner |
| `clippy` | `cargo clippy -- -D warnings` for both |
| `deny` | Security advisories, licenses, bans |
| `build-and-test` | Build + QEMU integration tests (11 tests, 2 boots) |

All jobs use Cargo. PR-only (no duplicate run on merge). Markdown-only PRs skip the full pipeline and run `markdownlint-cli2` instead.

## Architecture

See [DESIGN.md](DESIGN.md) for the full build system architecture, dependency graph, boot process, memory layout, and module reference.

## Documentation

- [DESIGN.md](DESIGN.md) — Build systems, architecture, CI pipeline, boot process
- [AGENTS.md](AGENTS.md) — Agent guidance, coding principles, build/test commands
