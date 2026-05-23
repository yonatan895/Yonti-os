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
cd kernel && cargo build --target x86_64-unknown-none   # compile kernel
cd runner && cargo build --no-default-features           # build runner + test-runner
cd runner && cargo run --bin runner -- bios              # run in QEMU
./run_tests.sh                                           # all tests (11 tests, 2 boots)
./run_tests.sh all                                       # unified test (10 tests, 1 boot)
```

## Testing

Tests are unified into two QEMU boots:

| Binary | Tests | Mechanism |
|--------|-------|-----------|
| `all_tests_elf` | 10 (basic_boot 1 + heap 4 + fs 5) | Single entry in `tests/all.rs`, shared init, `custom_test_frameworks` |
| `should_panic_elf` | 1 | Standalone, `harness=false`, expects kernel panic |

`run_tests.sh` builds ELFs (Bazel locally, Cargo in CI), then `test-runner` wraps each → BIOS image → QEMU (`isa-debug-exit`: 33=pass, 35=fail).

**Critical test quirk:** Tests using `custom_test_frameworks` and `reexport_test_harness_main` must use cargo's **default harness** (`harness = true`). Only `should_panic` and `stack_overflow` override this.

`stack_overflow` is flaky — skipped by default.

## CI/CD

PRs to `master` are gated by four jobs (PR-only, no duplicate on merge):

| Job | What it checks |
|-----|---------------|
| `fmt` | `cargo fmt --check` kernel + runner |
| `clippy` | `cargo clippy -- -D warnings` both workspaces |
| `deny` | `cargo deny check` both workspaces |
| `build-and-test` | Full Cargo build + `./run_tests.sh` (11 tests) |

Markdown-only PRs skip all CI checks — they require only a human review.

### Local pre-push checklist

```sh
cargo fmt --all -- && (cd runner && cargo fmt --)
(cd kernel && cargo clippy --target x86_64-unknown-none -- -D warnings)
(cd runner && cargo clippy --no-default-features -- -D warnings)
./run_tests.sh
cargo deny check && (cd runner && cargo deny check)
```

## Branching and PR workflow

Always work on a new branch off `master`. Never push directly to `master`, reuse merged branches, or force-push.

```sh
git checkout master && git pull origin master
git checkout -b feature/<description>    # or fix/<description>
# ... make changes ...
cargo fmt --all -- && (cd runner && cargo fmt --)
./run_tests.sh
git add -A && git commit -m "type: description"
git push origin feature/<description>
gh pr create --base master --title "..." --body "..."
```

Branch naming: `feature/<name>`, `fix/<name>`, `refactor/<name>`, `ci/<name>`, `docs/<name>`.

## Coding Principles

All changes must adhere to these principles:

1. **Test everything.** Every feature must include suitable test coverage. Integration tests run in QEMU via `#[test_case]` in `tests/common/`. Unit-testable logic should be factored out of `no_std` modules where possible.

2. **Assert preconditions and postconditions.** Validate all function arguments on entry. Validate return values at call sites. Use `assert!`, `assert_eq!`, and `debug_assert!` liberally. Panics are preferable to silent corruption in a kernel.

3. **Use explicitly-sized types.** Prefer `u8`, `u16`, `u32`, `u64` over architecture-dependent `usize`/`isize`. The kernel targets x86_64, but explicit sizes prevent portability bugs and make intent clear. Exception: `usize` is acceptable for memory addresses and indexing where the platform width is correct.

4. **Assert the positive and the negative.** For every invariant you enforce (e.g., "this pointer is non-null"), also assert the negative space you exclude (e.g., "this region does not overlap with the heap"). Defensive programming catches bugs before they become triple faults.

5. **Treat all compiler warnings as errors.** The CI pipeline uses `-D warnings`. Never suppress a warning without understanding it. If a warning must be allowed, use the narrowest possible `#[allow(...)]` with a comment explaining why.

6. **Optimize for the slowest resource first.** Network > disk > memory > CPU. In a kernel with no network and no disk, the priority is **memory bandwidth and allocation latency** over CPU cycles. Prefer compact data structures, bounded allocations, and cache-line-aligned hot paths.

7. **Use simple, explicit control flow.** Avoid deeply nested conditionals. Use early returns. Make every branch's purpose obvious. If a function has more than 3 levels of indentation, refactor it.

8. **No recursion. All loops must be bounded.** The kernel stack is 20 pages (80 KiB). Recursion or unbounded loops will overflow it. Every `loop {}` or `while` must have a provable exit condition or a maximum iteration count.

9. **Put a limit on everything and fail fast.** Every allocation, every buffer, every queue — define a maximum size. When the limit is exceeded, fail immediately with a clear error rather than silently degrading. A crashed kernel is easier to debug than a corrupt one.

## Architecture

### Build pipeline
1. `cargo` compiles the kernel ELF for `x86_64-unknown-none` via `build-std` (`kernel/.cargo/config.toml`)
2. The `runner` crate's `build.rs` wraps the ELF into bootable BIOS and UEFI disk images via `DiskImageBuilder`
3. QEMU boots with `-nographic -no-reboot -device isa-debug-exit,iobase=0xf4,iosize=0x04`

### Workspace structure
```
Yonti-os/                      # workspace root
├── Cargo.toml                 # workspace: members = ["kernel"]
├── MODULE.bazel               # Bazel module (local dev only)
├── kernel/                    # workspace member (bare-metal kernel)
│   ├── Cargo.toml             # bootloader_api 0.11, spin, x86_64, log, etc.
│   ├── .cargo/config.toml     # build-std, target = "x86_64-unknown-none"
│   ├── src/
│   │   ├── main.rs            # kernel_main entry, boot sequence, executor
│   │   ├── lib.rs             # crate root, init(), test framework, QEMU exit
│   │   ├── log.rs             # structured logging via log crate (error→trace)
│   │   ├── monitor.rs         # lock-free atomic metrics counters
│   │   ├── trace.rs           # 4096-entry execution trace ring buffer
│   │   ├── debug.rs           # crash dump: registers, stack trace, hexdump
│   │   ├── allocator.rs       # global allocator (TLSF), Locked<A>, init_heap
│   │   ├── allocator/
│   │   │   ├── tlsf.rs        # TLSF O(1) heap (active, alignment support)
│   │   │   ├── bump.rs, fixed_size_block.rs, linked_list.rs  # reference
│   │   ├── memory.rs          # OffsetPageTable init
│   │   ├── memory/buddy.rs    # buddy frame allocator (MAX_ORDER=10)
│   │   ├── uart.rs            # UART 16550 driver (replaces uart_16550 crate)
│   │   ├── pic.rs             # 8259 PIC driver (replaces pic8259 crate)
│   │   ├── serial.rs          # serial_print! macros
│   │   ├── vga_buffer.rs      # println! / print! macros
│   │   ├── framebuffer.rs     # pixel text renderer, 8x16 font
│   │   ├── font.rs            # bitmap font (96 glyphs, 1536 bytes)
│   │   ├── gdt.rs             # GDT/TSS (bootloader provides)
│   │   ├── interrupts.rs      # IDT, keyboard/timer/page fault handlers
│   │   ├── sse.rs             # SSE enablement
│   │   ├── array_queue.rs     # lock-free SPSC queue (replaces crossbeam-queue)
│   │   ├── async_utils.rs     # Stream, AtomicWaker (replaces futures-util)
│   │   ├── once_cell.rs       # OnceCell (replaces conquer-once)
│   │   ├── task/mod.rs        # Task, TaskId
│   │   ├── task/executor.rs   # async executor (BTreeMap, ArrayQueue, HLT idle)
│   │   ├── task/keyboard.rs   # async scancode stream
│   │   ├── fs/mod.rs          # FileSystem (global FS lazy_static)
│   │   └── fs/inode.rs         # Inode, InodeKind (File | Directory)
│   └── tests/
│       ├── all.rs             # unified test entry (10 tests, 1 boot)
│       ├── common/            # test function modules (basic_boot, heap, fs)
│       ├── should_panic.rs    # standalone (harness=false)
│       └── stack_overflow.rs  # standalone (harness=false, skipped)
├── runner/                    # standalone workspace (host target)
│   ├── Cargo.toml             # separate [workspace], bootloader dep
│   ├── build.rs               # builds kernel, creates BIOS/UEFI disk images
│   └── src/
│       ├── main.rs            # QEMU launcher (bios/uefi modes)
│       └── test_runner.rs     # wraps test ELF → bootable image → QEMU
├── .github/workflows/         # ci.yml, opencode.yml
├── deny.toml                  # cargo-deny configuration
├── run_tests.sh               # convenience test script
├── DESIGN.md                  # architecture & build system reference
└── README.md
```

### Memory
```
Physical:  Buddy allocator (4 KiB–4 MiB, bitmap, MAX_ORDER=10)
              ↓ allocate_frame()
Virtual:   TLSF heap at 0x4444_4444_0000 (1 MiB + sentinel page, O(1))
              ↓ GlobalAlloc
           Box, Vec, String, BTreeMap, all alloc::*
```

### Observability pipeline
- `log.rs`: leveled macros (`error!`–`trace!`), compile-time filtering, serial output
- `monitor.rs`: atomic counters for alloc, frames, tasks, interrupts, timer ticks
- `trace.rs`: 4096-entry ring buffer, RDTSC timestamps, `trace_event!` macro
- `debug.rs`: full crash dump (registers + stack trace + metrics + last 16 trace events)

## Inline driver modules

These replace external crates (32 → 15 in Cargo.lock):

| Module | Replaces | Eliminated |
|--------|----------|------------|
| `uart.rs` | `uart_16550` | x86_64 0.14 duplicate |
| `pic.rs` | `pic8259` | x86_64 0.14 duplicate |
| `array_queue.rs` | `crossbeam-queue` | 4 transitive deps |
| `async_utils.rs` | `futures-util` | 4 transitive deps |
| `once_cell.rs` | `conquer-once` | 1 transitive dep |

## Dependency API gotchas

- **`bootloader_api` 0.11.x**: `entry_point!` macro with `config = &BOOTLOADER_CONFIG`. `BootInfo` has `memory_regions`, `physical_memory_offset.into_option()`, `framebuffer.take()`.
- **`x86_64` 0.15.4+**: `gdt.push()` is private → use `gdt.append()`. `idt.interrupts[index]` is private → use `idt[index]`.
- **`spin` 0.9.x**: `spin::Mutex`, not `std::sync::Mutex`. Features: `spin_mutex`, `once`, `lock_api`.
- **`linked_list_allocator` 0.10.x**: `Heap::init()` takes `*mut u8`, not `usize`.

## Runtime behavior

If the kernel hangs or triple-faults silently, check:
- Missing `#[global_allocator]` init — heap must be initialized before any allocation
- Page table mappings — `init_heap` maps 257 pages at `0x4444_4444_0000`
- TLSF sentinel page — the extra page at heap end must be mapped
- Framebuffer init before first `println!` — before init it's a no-op
- AtomicWaker race — `register()` and `take()` use `without_interrupts()` guard

## Commit conventions

```
type: description
```

Where type is: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `chore`.

Branch naming: `feature/<description>` or `fix/<description>`.
