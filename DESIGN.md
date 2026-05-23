# Yonti-os — Build Systems, Architecture & Module Reference

Bare-metal x86_64 kernel in Rust (edition 2024, nightly, MIT). 22 public modules, 46 tests in 2 QEMU boots, Cargo build system.

---

## Workspace Structure

```
Yonti-os/
├── Cargo.toml              # Workspace root: members = ["kernel"]
├── kernel/                  # Workspace member: bare-metal kernel
│   ├── src/
│   │   ├── lib.rs           # Crate root, init(), test framework, QEMU exit
│   │   ├── main.rs          # kernel_main: boot, heap, FS demo, executor
│   │   ├── allocator.rs     # Global allocator (TLSF), Locked<A>
│   │   ├── allocator/       # tlsf.rs, bump.rs, linked_list.rs
│   │   ├── memory.rs        # OffsetPageTable init, EmptyFrameAllocator
│   │   ├── memory/buddy.rs  # Buddy physical frame allocator
│   │   ├── uart.rs          # UART 16550 driver (replaces uart_16550 crate)
│   │   ├── pic.rs           # 8259 PIC driver (replaces pic8259 crate), mask_all()
│   │   ├── apic/mod.rs      # APIC subsystem: ACPI MADT parser, detection, init
│   │   ├── apic/lapic.rs    # LAPIC MMIO driver (ID, version, SVR, EOI)
│   │   ├── apic/ioapic.rs   # I/O APIC MMIO driver (indirect registers, redirection)
│   │   ├── serial.rs        # serial_print! / serial_println! macros
│   │   ├── vga_buffer.rs    # println! / print! macros (serial + framebuffer)
│   │   ├── framebuffer.rs   # Pixel text renderer, 8×16 font, Ctrl+/- scaling
│   │   ├── font.rs          # Auto-generated bitmap font (96 glyphs)
│   │   ├── gdt.rs           # GDT/TSS (bootloader provides)
│   │   ├── interrupts.rs    # IDT, conditional APIC/PIC EOI dispatch
│   │   ├── sse.rs           # SSE enablement via CR0/CR4
│   │   ├── shell.rs         # Async shell with command dispatch, keyboard I/O
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
│       ├── all.rs           # Unified test entry (46 tests, 1 boot)
│       ├── common/          # Test function modules (7 modules: apic, array_queue,
│       │                    #   basic_boot, buddy_allocator, file_system,
│       │                    #   framebuffer, heap_allocation)
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
│   └── opencode.yml         # AI assistant trigger
├── deny.toml                # cargo-deny config (advisories, licenses, bans)
├── run_tests.sh             # Build + test script
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

**Kernel dependencies** (14 crates in lock file):
`bootloader_api` 0.11, `x86_64` 0.15, `spin` 0.9 (`spin_mutex`, `once`, `lock_api`), `lazy_static` 1.5 (`spin_no_std`), `pc-keyboard` 0.9, `log` 0.4 (no_std, info max).

**Runner dependencies** (30–110 crates):
`bootloader` 0.11 (build + runtime), `ovmf-prebuilt` 0.2 (optional, `uefi` feature). Feature-gated: `--no-default-features` in CI drops ~80 transitive crates.



---

## Kernel Module Reference (22 public modules)

### Core Infrastructure (6)

| Module | LOC | Purpose |
|--------|-----|---------|
| `lib.rs` | 132 | Crate root, `BOOTLOADER_CONFIG`, `init()` (SSE/IDT/PIC), test framework, QEMU exit |
| `main.rs` | 105 | `kernel_main`: boot sequence, FS demo, executor spawn |
| `gdt.rs` | 55 | GDT/TSS + double-fault IST stack via `SyncUnsafeCell` |
| `interrupts.rs` | 107 | IDT, conditional APIC/PIC EOI dispatch; page fault triggers crash dump via `panic!` |
| `apic/mod.rs` | 373 | APIC subsystem: inline ACPI MADT parser (~150 LOC), detection, LAPIC+I/O APIC init, IRQ→GSI mapping |
| `apic/lapic.rs` | 113 | LAPIC MMIO driver (ID, version, SVR enable, EOI at offset 0xB0) |
| `apic/ioapic.rs` | 132 | I/O APIC MMIO driver (indirect register access, 24-entry redirection table) |
| `sse.rs` | 25 | SSE enablement via CR0/CR4 registers |

### Memory Management (6)

| Module | LOC | Purpose |
|--------|-----|---------|
| `memory.rs` | 55 | `OffsetPageTable` init, `EmptyFrameAllocator` |
| `memory/buddy.rs` | 326 | Buddy frame allocator (4 KiB–4 MiB, bitmap, deallocation) |
| `allocator.rs` | 106 | `#[global_allocator]`, `Locked<A>`, `init_heap()` |
| `allocator/tlsf.rs` | 347 | TLSF heap (O(1), 19×32 classes, coalescing, alignment), `debug_assert!` guards against 4 GiB truncation |
| `allocator/bump.rs` | 64 | Bump allocator (reference) |
| `allocator/linked_list.rs` | 154 | Linked-list allocator (reference) |

### I/O & Display (7)

| Module | LOC | Purpose |
|--------|-----|---------|
| `uart.rs` | 99 | UART 16550 driver, bounded `wait_for!` spin loop (100K retries) |
| `pic.rs` | 120 | 8259 PIC driver (replaces `pic8259` crate) |
| `serial.rs` | 41 | `serial_print!` macros, serial port init |
| `vga_buffer.rs` | 18 | `println!`/`print!` macros (serial + framebuffer) |
| `framebuffer.rs` | 366 | Pixel text renderer, scrolling, RGB/BGR, Ctrl+/- text scaling (1×–4×), ANSI SGR colors, underline cursor |
| `font.rs` | 108 | 8×16 VGA font (96 glyphs, 1536 bytes) |
| `shell.rs` | 230 | Async command shell: help, mem, trace, ls, cat, alloc, uptime, clear, echo |

### Async Runtime (5)

| Module | LOC | Purpose |
|--------|-----|---------|
| `task/mod.rs` | 46 | `Task` struct, `TaskId`, manual `Debug` impl |
| `task/executor.rs` | 116 | Async executor: `BTreeMap`, `ArrayQueue`, HLT idle, manual `Debug` impl |
| `task/keyboard.rs` | 92 | Async scancode stream, interrupt-safe |
| `array_queue.rs` | 85 | Lock-free SPSC ring buffer (replaces `crossbeam-queue`) |
| `async_utils.rs` | 91 | `Stream`, `StreamExt`, `AtomicWaker` with interrupt guard (replaces `futures-util`) |
| `once_cell.rs` | 59 | `OnceCell` (replaces `conquer-once`) |

### Filesystem (2)

| Module | LOC | Purpose |
|--------|-----|---------|
| `fs/mod.rs` | 234 | Hierarchical in-memory FS, `static FS` (no `lazy_static`), `&'static str` paths |
| `fs/inode.rs` | 102 | `Inode` with `&'static str` names, const constructors, `#[derive(Debug)]` on `FileSystem` |

### Observability (4)

| Module | LOC | Purpose |
|--------|-----|---------|
| `log.rs` | 76 | Structured logging via `log` crate (error→trace levels) |
| `monitor.rs` | 199 | Lock-free atomic counters: alloc, frames, tasks, interrupts |
| `trace.rs` | 144 | 4096-entry ring buffer, RDTSC timestamps, `trace_event!` macro |
| `debug.rs` | 140 | Register dump (16 GPRs + CR0–CR4), RBP-chain stack trace, crash dump, hexdump |

---

## Unified Test Binary

Tests are split into two QEMU boots:

| Binary | Tests | Boot Mechanism |
|--------|-------|----------------|
| `all_tests_elf` | 46 (basic_boot 1, heap 7, fs 7, framebuffer 11, buddy 5, array_queue 3, apic 12) | Shared framebuffer + heap + TLSF init, runs all `#[test_case]` fns |
| `should_panic_elf` | 1 | Standalone, `harness=false`, expects kernel panic |
| `stack_overflow` | 1 | Skipped by default (triggers real stack overflow, flaky) |

**Before unification:** 4 separate binaries → 4 QEMU boots, ~93s.
**After unification:** 2 binaries → 2 QEMU boots, ~49s (47% faster).

### Test flow

```
run_tests.sh
  ├─ cargo build --tests --target x86_64-unknown-none
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
| `build-and-test` | Cargo + QEMU | Build runner + test ELFs, upload boot images artifact, 46 tests in 2 boots |

**Caching:** Shared `cargo-*` key. Paths: `~/.cargo/registry/`, `~/.cargo/git/`, `target/`, `runner/target/`.

---

## Boot Process

```
QEMU → SeaBIOS → Bootloader stages 2–4
  → kernel_main()
    → yonti_os::init()              SSE, IDT, PIC (interrupts NOT enabled yet)
    → framebuffer::init()           pixel text renderer (white on black)
    → log::init(LevelFilter::Info)  structured logging to serial
    → BuddyAllocator::new()         physical frame allocator (from memory map)
    → init_heap()                   map 257 pages, init TLSF at 0x4444_4444_0000
    → trace::init()                 event ring buffer
    → APIC init sequence            detect (ACPI MADT), init (LAPIC + I/O APIC),
    │                               mask PIC on success, enable interrupts
    → demo_fs()                     create files/dirs, write/read data
    → Executor::run()               async tasks (shell_task + example_task), HLT idle
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

## Dependency Graph (Kernel, 14 crates)

```
yonti_os (root)
├── bootloader_api 0.11      entry_point!, BootInfo, FrameBufferInfo, rsdp_addr
├── x86_64 0.15              port I/O, paging, IDT, GDT, MSR
│   ├── bit_field, bitflags, volatile
├── spin 0.9                 Mutex, Once, RwLock
│   ├── lock_api, scopeguard
├── lazy_static 1.5          Static initialization (GDT/TSS, IDT)
├── pc-keyboard 0.9          Scancode decoding (set 1, US layout)
├── log 0.4                  Log facade (no_std, info max)
├── const_fn 0.4             (proc-macro, build-time)
└── rustversion 1.0          (proc-macro, build-time)

Inline modules (replaced external crates):
  uart.rs ← uart_16550     pic.rs ← pic8259
  array_queue.rs ← crossbeam-queue (+4 transitive)
  async_utils.rs ← futures-util (+4 transitive)
  once_cell.rs ← conquer-once (+1 transitive)

Net reduction: 32 → 14 crates (56% fewer)
```

---

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|

| TLSF as global allocator | O(1) guarantee, better fragmentation than fixed-size block allocator |
| Buddy frame allocator | Enables frame deallocation (prerequisite for slab allocator) |
| Inline driver modules | Eliminated 18 crates, zero duplicate versions |
| Unified test binary | 4 QEMU boots → 2 (93s → 49s, 47% faster) |
| Observability pipeline | log → monitor → trace → debug, each builds on the prior |
| `--cfg bazel` guard in lib.rs | Bazel compiles library with test API but without entry_point |
| AtomicWaker interrupt guard | `without_interrupts()` prevents ISR/task data race |
| buddy NULL_LINK = usize::MAX | Frame index 0 is valid; zero sentinel would truncate free lists |
| Bounded UART `wait_for!` | 100K retries prevents infinite spin if UART hardware hangs |
| Page fault → `panic!` | Triggers crash dump (registers, stack trace, metrics, trace events) |
| `SyncUnsafeCell` IST stack | Replaces `static mut` for double-fault stack; multicore-ready with `#![feature(sync_unsafe_cell)]` |
| APIC with PIC fallback | Inline ACPI MADT parser detects LAPIC+I/O APIC; PIC stays as fallback if no MADT/I/O APIC found |
| `static FS` (no `lazy_static`) | `&'static str` Inode names + const constructors eliminate one-time init overhead |
| `#[derive(Debug)]` on public types | `FileSystem`, `TlsfAllocator`, `BuddyAllocator` provide useful panic diagnostics |
| DRY `align_up` | Single `pub(crate)` definition in `allocator.rs` with `debug_assert!(is_power_of_two)` |
| TLSF 4 GiB guard | `debug_assert!` on `u32` truncation; module-level doc warns about heap size limits |

---

## Quick Reference

| Operation | Cargo |
|-----------|-------|
| Build kernel | `cd kernel && cargo build --target x86_64-unknown-none` |
| Build test ELFs | `cargo build --tests --target x86_64-unknown-none` |
| Run all tests | `./run_tests.sh` |
| Run single test | `./run_tests.sh all` |
| Format check | `cargo fmt --all -- --check` |
| Clippy check | `cd kernel && cargo clippy --target x86_64-unknown-none -- -D warnings` |
| Deny check | `cargo deny check` |
| Clean | `cargo clean` |
