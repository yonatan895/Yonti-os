# Yonti-os — Build Systems, Architecture & Module Reference

Bare-metal x86_64 kernel in Rust (edition 2021, nightly, MIT). 20 public modules, 11 tests in 2 QEMU boots, dual build system (Cargo + Bazel).

---

## Workspace Structure

```
Yonti-os/
├── Cargo.toml              # Workspace root: members = ["kernel"]
├── MODULE.bazel             # Bazel module: rules_rust 0.70.0, crate_universe
├── .bazelrc                 # Config: --config=bare, --config=host, nightly
├── .bazelversion            # Pinned Bazel 7.4.1
├── BUILD.bazel              # Root convenience targets (fmt, clippy, deny)
├── platforms/
│   └── BUILD.bazel          # x86_64_bare_metal, x86_64_linux
├── tools/
│   ├── BUILD.bazel          # qemu_runner host binary
│   ├── qemu_runner.rs       # Wraps kernel ELF → bootable image → QEMU
│   ├── qemu_test.bzl        # Custom Starlark rule for QEMU kernel tests
│   └── deny.sh              # cargo-deny wrapper for Bazel
├── kernel/                  # Workspace member: bare-metal kernel
│   ├── Cargo.toml           # Deps: bootloader_api, spin, x86_64, log, etc.
│   ├── .cargo/config.toml   # build-std, target = x86_64-unknown-none
│   ├── BUILD.bazel          # rust_library + rust_binary + test ELFs
│   ├── src/
│   │   ├── lib.rs           # Crate root, init(), test framework, QEMU exit
│   │   ├── main.rs          # kernel_main: boot, heap, FS demo, executor
│   │   ├── allocator.rs     # Global allocator (TLSF), Locked<A>
│   │   ├── allocator/       # tlsf.rs, bump.rs, fixed_size_block.rs, linked_list.rs
│   │   ├── memory.rs        # OffsetPageTable init, EmptyFrameAllocator
│   │   ├── memory/buddy.rs  # Buddy physical frame allocator
│   │   ├── uart.rs          # UART 16550 driver (replaces uart_16550 crate)
│   │   ├── pic.rs           # 8259 PIC driver (replaces pic8259 crate)
│   │   ├── serial.rs        # serial_print! / serial_println! macros
│   │   ├── vga_buffer.rs    # println! / print! macros (serial + framebuffer)
│   │   ├── framebuffer.rs   # Pixel text renderer, 8×16 font
│   │   ├── font.rs          # Auto-generated bitmap font (96 glyphs)
│   │   ├── gdt.rs           # GDT/TSS (bootloader provides)
│   │   ├── interrupts.rs    # IDT, PIC handlers: timer, keyboard, exceptions
│   │   ├── sse.rs           # SSE enablement via CR0/CR4
│   │   ├── task/mod.rs      # Task struct, TaskId
│   │   ├── task/executor.rs # Async executor (BTreeMap, ArrayQueue, HLT idle)
│   │   ├── task/keyboard.rs # Async scancode stream via AtomicWaker + ArrayQueue
│   │   ├── array_queue.rs   # Lock-free SPSC ring buffer (replaces crossbeam-queue)
│   │   ├── async_utils.rs   # Stream, StreamExt, AtomicWaker (replaces futures-util)
│   │   ├── once_cell.rs     # OnceCell (replaces conquer-once)
│   │   ├── fs/mod.rs        # Hierarchical in-memory filesystem
│   │   ├── fs/inode.rs      # Inode, InodeKind (File, Directory)
│   │   ├── log.rs           # Structured logging via log crate facade
│   │   ├── monitor.rs       # Lock-free atomic metrics (alloc, task, interrupt)
│   │   ├── trace.rs         # 4096-entry execution tracing ring buffer
│   │   └── debug.rs         # Crash diagnostics: registers, stack trace, hexdump
│   └── tests/
│       ├── all.rs           # Unified test entry (10 tests, 1 boot)
│       ├── common/          # Test function modules (no entry_point)
│       ├── should_panic.rs  # Standalone panic-expected test
│       └── stack_overflow.rs
├── runner/                  # Standalone Cargo workspace (NOT a Bazel member)
│   ├── Cargo.toml           # [workspace], bootloader, ovmf-prebuilt (optional)
│   ├── build.rs             # Builds kernel, creates BIOS/UEFI disk images
│   └── src/
│       ├── main.rs          # QEMU launcher (bios/uefi modes)
│       └── test_runner.rs   # Wraps test ELF → bootable image → QEMU
├── .github/workflows/
│   ├── ci.yml               # Main CI: fmt, clippy, deny, build-and-test
│   ├── markdown-lint.yml    # Markdown lint for .md-only PRs
│   └── opencode.yml         # AI assistant trigger
├── deny.toml                # cargo-deny config (advisories, licenses, bans)
├── run_tests.sh             # Build + test (Bazel ELFs locally, Cargo in CI)
└── AGENTS.md                # Agent guidance + coding principles
```

---

## Build Systems

### Cargo (primary, used in CI)

Two separate Cargo workspaces:

| Workspace | Directory | Target | Key Config |
|-----------|-----------|--------|------------|
| Kernel | `kernel/` | `x86_64-unknown-none` | `build-std = ["core", "compiler_builtins", "alloc"]`, `panic = "abort"` |
| Runner | `runner/` | `x86_64-unknown-linux-gnu` | `[workspace]` (separate), `--no-default-features` in CI |

**Kernel dependencies** (15 crates in lock file):
`bootloader_api` 0.11, `x86_64` 0.15, `spin` 0.9 (`spin_mutex`, `once`, `lock_api`), `lazy_static` 1.0 (`spin_no_std`), `pc-keyboard` 0.5, `linked_list_allocator` 0.10, `log` 0.4 (no_std, info max).

**Runner dependencies** (30–110 crates):
`bootloader` 0.11 (build + runtime), `ovmf-prebuilt` 0.2 (optional, `uefi` feature). Feature-gated: `--no-default-features` in CI drops ~80 transitive crates.

### Bazel (local dev, hermetic)

- **rules_rust** 0.70.0, Bazel 7.4.1, nightly pinned to `2026-05-21`
- Toolchain: `extra_rustc_flags = ["-C", "linker=rust-lld"]`
- `crate.from_cargo()` bridges Cargo.lock → Bazel deps (two separate calls for kernel + runner)
- Platforms: `x86_64_bare_metal` (`@platforms//os:none`) and `x86_64_linux`
- Bazel is NOT used in CI — Cargo is faster with proven caching; Bazel is for local development

**Usage:**
```sh
bazel build --config=bare //kernel:yonti_os         # kernel ELF
bazel build --config=bare //kernel:all_tests_elf    # test ELFs
bazel build //:fmt                                   # format check
bazel build //:clippy                                # clippy check
```

---

## Kernel Module Reference (20 public modules)

### Core Infrastructure (5)

| Module | LOC | Purpose |
|--------|-----|---------|
| `lib.rs` | 130 | Crate root, `BOOTLOADER_CONFIG`, `init()` (SSE/IDT/PIC), test framework, QEMU exit |
| `main.rs` | 105 | `kernel_main`: boot sequence, FS demo, executor spawn |
| `gdt.rs` | 54 | GDT/TSS setup (bootloader 0.11 provides by default) |
| `interrupts.rs` | 114 | IDT, keyboard/timer/page-fault handlers, PIC EOI |
| `sse.rs` | 23 | SSE enablement via CR0/CR4 registers |

### Memory Management (6)

| Module | LOC | Purpose |
|--------|-----|---------|
| `memory.rs` | 49 | `OffsetPageTable` init, `EmptyFrameAllocator` |
| `memory/buddy.rs` | 299 | Buddy frame allocator (4 KiB–4 MiB, bitmap, deallocation) |
| `allocator.rs` | 104 | `#[global_allocator]`, `Locked<A>`, `init_heap()` |
| `allocator/tlsf.rs` | 338 | TLSF heap (O(1), 19×32 classes, coalescing, alignment support) |
| `allocator/bump.rs` | 64 | Bump allocator (reference) |
| `allocator/fixed_size_block.rs` | 103 | Fixed-size block allocator (reference) |
| `allocator/linked_list.rs` | 148 | Linked-list allocator (reference) |

### I/O & Display (6)

| Module | LOC | Purpose |
|--------|-----|---------|
| `uart.rs` | 107 | UART 16550 driver (replaces `uart_16550` crate) |
| `pic.rs` | 114 | 8259 PIC driver (replaces `pic8259` crate) |
| `serial.rs` | 41 | `serial_print!` macros, serial port init |
| `vga_buffer.rs` | 18 | `println!`/`print!` macros (serial + framebuffer) |
| `framebuffer.rs` | 150 | Pixel text renderer, scrolling, RGB/BGR |
| `font.rs` | 108 | 8×16 VGA font (96 glyphs, 1536 bytes) |

### Async Runtime (5)

| Module | LOC | Purpose |
|--------|-----|---------|
| `task/mod.rs` | 35 | `Task` struct, `TaskId` atomic counter |
| `task/executor.rs` | 111 | Async executor: `BTreeMap`, `ArrayQueue`, HLT idle |
| `task/keyboard.rs` | 86 | Async scancode stream, interrupt-safe |
| `array_queue.rs` | 81 | Lock-free SPSC ring buffer (replaces `crossbeam-queue`) |
| `async_utils.rs` | 91 | `Stream`, `StreamExt`, `AtomicWaker` with interrupt guard (replaces `futures-util`) |
| `once_cell.rs` | 61 | `OnceCell` (replaces `conquer-once`) |

### Filesystem (2)

| Module | LOC | Purpose |
|--------|-----|---------|
| `fs/mod.rs` | 156 | Hierarchical in-memory FS, `lazy_static` global `FS` |
| `fs/inode.rs` | 94 | `Inode`, `InodeKind` (File/Directory), BTreeMap children |

### Observability (4)

| Module | LOC | Purpose |
|--------|-----|---------|
| `log.rs` | 69 | Structured logging via `log` crate (error→trace levels) |
| `monitor.rs` | 186 | Lock-free atomic counters: alloc, frames, tasks, interrupts |
| `trace.rs` | 140 | 4096-entry ring buffer, RDTSC timestamps, `trace_event!` macro |
| `debug.rs` | 140 | Register dump (16 GPRs + CR0–CR4), RBP-chain stack trace, crash dump, hexdump |

---

## Unified Test Binary

Tests are split into two QEMU boots:

| Binary | Tests | Boot Mechanism |
|--------|-------|----------------|
| `all_tests_elf` | 10 (basic_boot 1, heap 4, fs 5) | Shared framebuffer + heap + TLSF init, runs all `#[test_case]` fns |
| `should_panic_elf` | 1 | Standalone, `harness=false`, expects kernel panic |
| `stack_overflow` | 1 | Skipped by default (triggers real stack overflow, flaky) |

**Before unification:** 4 separate binaries → 4 QEMU boots, ~93s.
**After unification:** 2 binaries → 2 QEMU boots, ~49s (47% faster).

### Test flow

```
run_tests.sh
  ├─ bazel build --config=bare //kernel:all_tests_elf     (local) OR
  │  cargo build --tests --target x86_64-unknown-none      (CI)
  ├─ cargo build --no-default-features --bin test-runner
  └─ for each ELF:
       test-runner <elf> → DiskImageBuilder → BIOS image → QEMU
       (isa-debug-exit: 33=pass, 35=fail)
```

---

## CI Pipeline

Triggered on PRs to `master` (not on merge). Markdown-only PRs skip this pipeline.

| Job | Technology | Check |
|-----|-----------|-------|
| `fmt` | Cargo | `cargo fmt --check` kernel + runner |
| `clippy` | Cargo | Kernel: `cargo clippy --target x86_64-unknown-none -- -D warnings`. Runner: `SKIP_KERNEL_BUILD=1 cargo clippy --no-default-features -- -D warnings` |
| `deny` | cargo-deny | Advisories, licenses, bans for both workspaces |
| `build-and-test` | Cargo + QEMU | Build runner + test ELFs, upload boot images artifact, 11 tests in 2 boots |

**Caching:** Shared `cargo-*` key. Paths: `~/.cargo/registry/`, `~/.cargo/git/`, `target/`, `runner/target/`.

---

## Boot Process

```
QEMU → SeaBIOS → Bootloader stages 2–4
  → kernel_main()
    → yonti_os::init()              SSE, IDT, PIC, enable interrupts
    → framebuffer::init()           pixel text renderer (white on black)
    → log::init(LevelFilter::Info)  structured logging to serial
    → BuddyAllocator::new()         physical frame allocator (from memory map)
    → init_heap()                   map 257 pages, init TLSF at 0x4444_4444_0000
    → trace::init()                 event ring buffer
    → demo_fs()                     create files/dirs, write/read data
    → Executor::run()               async tasks (keyboard + example), HLT idle
```

---

## Memory Layout

```
Virtual address space:
  0x0000_0000_0000    Kernel code + data (loaded by bootloader)
  0x4444_4444_0000    HEAP_START — TLSF heap, 1 MiB + sentinel page
  0x4444_4454_1000    End of mapped heap region (257 pages)

Physical memory:
  Buddy allocator manages usable frames (MAX_ORDER=10, 4 KiB–4 MiB)
  MAX_TRACKED_FRAMES: 131,072 (512 MiB RAM)
  NULL_LINK = usize::MAX (sentinel for free-list links)
```

---

## Dependency Graph (Kernel, 15 crates)

```
yonti_os (root)
├── bootloader_api 0.11      entry_point!, BootInfo, FrameBufferInfo
├── x86_64 0.15              port I/O, paging, IDT, GDT
│   ├── bit_field, bitflags, volatile
├── spin 0.9                 Mutex, Once
│   ├── lock_api, scopeguard
├── lazy_static 1.0          Static initialization
├── pc-keyboard 0.5          Scancode decoding
├── linked_list_allocator 0.10  Heap fallback
│   └── spinning_top
├── log 0.4                  Log facade (no_std, info max)
├── const_fn 0.4             (proc-macro, build-time)
└── rustversion 1.0          (proc-macro, build-time)

Inline modules (replaced external crates):
  uart.rs ← uart_16550     pic.rs ← pic8259
  array_queue.rs ← crossbeam-queue (+4 transitive)
  async_utils.rs ← futures-util (+4 transitive)
  once_cell.rs ← conquer-once (+1 transitive)

Net reduction: 32 → 15 crates (53% fewer)
```

---

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Dual build system | Bazel for hermetic local dev; Cargo for CI (faster caching, no cold start) |
| Separate runner workspace | Runner is host target; must not inherit kernel's `build-std` config |
| TLSF as global allocator | O(1) guarantee, better fragmentation than fixed-size block allocator |
| Buddy frame allocator | Enables frame deallocation (prerequisite for slab allocator) |
| Inline driver modules | Eliminated 17 crates, zero duplicate versions |
| Unified test binary | 4 QEMU boots → 2 (93s → 49s, 47% faster) |
| Observability pipeline | log → monitor → trace → debug, each builds on the prior |
| `--cfg bazel` guard in lib.rs | Bazel compiles library with test API but without entry_point |
| AtomicWaker interrupt guard | `without_interrupts()` prevents ISR/task data race |
| buddy NULL_LINK = usize::MAX | Frame index 0 is valid; zero sentinel would truncate free lists |

---

## Quick Reference

| Operation | Cargo | Bazel (local) |
|-----------|-------|---------------|
| Build kernel | `cd kernel && cargo build --target x86_64-unknown-none` | `bazel build --config=bare //kernel:yonti_os` |
| Build test ELFs | `cargo build --tests --target x86_64-unknown-none` | `bazel build --config=bare //kernel:all_tests_elf` |
| Run all tests | `./run_tests.sh` | `./run_tests.sh` |
| Run single test | `./run_tests.sh all` | same |
| Format check | `cargo fmt --all -- --check` | `bazel build //:fmt` |
| Clippy check | `cd kernel && cargo clippy --target x86_64-unknown-none -- -D warnings` | `bazel build //:clippy` |
| Deny check | `cargo deny check` | `bazel run //:deny` |
| Clean | `cargo clean` | `bazel clean` |
