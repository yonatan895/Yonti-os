# Yonti-os — Build System & Architecture

Bare-metal x86_64 kernel in Rust. Two build systems in parallel: **Cargo** (original) and **Bazel** (hermetic). Cargo owns dependency resolution (`Cargo.toml`/`Cargo.lock`); Bazel owns hermetic compilation and testing.

---

## Workspace Structure

```
Yonti-os/
├── Cargo.toml            # Workspace root: members = ["kernel"]
├── Cargo.lock            # Resolved deps for kernel workspace
├── MODULE.bazel           # Bazel module: rules_rust, crate_universe
├── .bazelrc               # Bazel config: --config=bare, --config=host
├── .bazelversion          # Pinned Bazel 7.4.1
├── BUILD.bazel            # Root convenience targets (fmt, clippy, deny)
├── platforms/
│   └── BUILD.bazel        # x86_64_bare_metal, x86_64_linux
├── tools/
│   ├── BUILD.bazel        # qemu_runner host binary
│   ├── qemu_test.bzl      # Custom Starlark rule for QEMU tests
│   ├── qemu_runner.rs     # Wraps kernel ELF → bootable image → QEMU
│   └── deny.sh            # cargo-deny wrapper
├── kernel/                # Workspace member: bare-metal kernel
│   ├── Cargo.toml         # bootloader_api, spin, x86_64, …
│   ├── .cargo/config.toml # build-std, target = x86_64-unknown-none
│   ├── BUILD.bazel        # Bazel: rust_library + rust_binary + test ELFs
│   ├── src/               # Kernel source (lib.rs, main.rs, modules)
│   └── tests/             # Integration test binaries
│       ├── all.rs          # Unified test entry (10 tests, 1 boot)
│       ├── common/         # Test function modules (pure, no entry_point)
│       ├── should_panic.rs # Standalone (harness=false)
│       └── stack_overflow.rs
├── runner/                # Standalone Cargo workspace (NOT a Bazel member)
│   ├── Cargo.toml         # [workspace], bootloader, ovmf-prebuilt
│   ├── .cargo/            # target = x86_64-unknown-linux-gnu
│   ├── build.rs           # Builds kernel + creates BIOS/UEFI disk images
│   └── src/
│       ├── main.rs         # QEMU launcher (bios/uefi modes)
│       └── test_runner.rs  # Wraps test ELF → bootable image → QEMU
├── .github/workflows/
│   └── ci.yml             # CI: fmt, clippy, deny, build-and-test
├── deny.toml              # cargo-deny config (advisories, licenses, bans)
├── run_tests.sh           # Build (Bazel) + test (Cargo test-runner)
└── AGENTS.md              # Agent guidance
```

---

## Cargo Build System (Original)

### Kernel compilation (`kernel/`)

```
┌──────────────────────────────────────────────────────────┐
│ kernel/.cargo/config.toml                                │
│   [build] target = "x86_64-unknown-none"                 │
│   [unstable] build-std = ["core","compiler_builtins",    │
│                            "alloc"]                      │
└─────────────────┬────────────────────────────────────────┘
                  │
                  ▼
         ┌───────────────┐
         │  cargo build   │  →  target/x86_64-unknown-none/
         │  --target      │      debug/yonti_os (ELF)
         │  x86_64-       │
         │  unknown-none  │
         └───────────────┘
```

- **`build-std`**: Compiles `core`, `compiler_builtins`, and `alloc` from source for the bare-metal target. This replaces the standard library.
- **Target**: `x86_64-unknown-none` — a built-in Rust target for freestanding x86_64.
- **Panic strategy**: `panic = "abort"` (set in workspace `Cargo.toml` profiles).

### Runner compilation (`runner/`)

```
┌────────────────────────────────────────────┐
│ runner/.cargo/config.toml                  │
│   [build] target = "x86_64-unknown-        │
│                      linux-gnu"            │
└─────────────┬──────────────────────────────┘
              │
              ▼
     ┌─────────────────┐
     │  cargo build     │ ──build.rs──→ builds kernel ELF
     │  --bin runner    │               creates BIOS/UEFI images
     │  --bin test-     │               via DiskImageBuilder
     │      runner      │
     └─────────────────┘
```

`runner/build.rs` is a build script that:
1. Invokes `cargo build --target x86_64-unknown-none` from `../kernel` to build the kernel ELF
2. Calls `DiskImageBuilder::new(kernel_elf).create_bios_image(path)` to produce a bootable disk image
3. Calls `DiskImageBuilder::new(kernel_elf).create_uefi_image(path)` for UEFI
4. Copies images to `runner/target/boot-images/` for CI artifact uploads
5. Exports `BIOS_IMG` and `UEFI_IMG` environment variables for the `runner` binary

### Dependency resolution

All dependencies flow through `Cargo.toml` → `Cargo.lock`. The kernel has 15 crates in its lock file (post-dependency-reduction), the runner has 30–110 depending on whether the `uefi` feature is enabled.

---

## Bazel Build System (New)

### Motivation

Bazel provides:
- **Hermetic builds**: Rust toolchain downloaded by Bazel, no host `rustup` needed
- **Reproducible**: pinned Bazel version + pinned nightly Rust
- **Advanced caching**: remote cache support, fine-grained incremental builds
- **Unified operations**: `bazel build`, `bazel test`, `bazel run` for everything

### Toolchain setup (`MODULE.bazel`)

```python
rust.toolchain(
    edition = "2021",
    versions = ["nightly/2026-05-21"],
    extra_target_triples = [
        "x86_64-unknown-none",       # bare-metal kernel
        "x86_64-unknown-linux-gnu",  # host (runner tools)
    ],
)
```

- **Hermetic Rust**: Bazel downloads `rustc`, `rust-std`, and `llvm-tools` from `static.rust-lang.org`. No `-Zbuild-std` needed — prebuilt `core`/`alloc` for `x86_64-unknown-none` is fetched automatically.
- **Pinned nightly**: `nightly/2026-05-21` — matches the `rust-toolchain.toml` version.

### Dependency bridging (`crate.from_cargo()`)

Bazel reads `Cargo.toml`/`Cargo.lock` via `crate_universe` and generates Bazel BUILD files:

```
Cargo.toml / Cargo.lock
        │
        ▼ crate.from_cargo()
┌───────────────────┐     ┌────────────────────┐
│ @crates_kernel    │     │ @crates_runner     │
│ (15 crates)       │     │ (30-110 crates)    │
├───────────────────┤     ├────────────────────┤
│ bootloader_api    │     │ bootloader         │
│ x86_64            │     │ ovmf-prebuilt      │
│ spin              │     │ fatfs, gpt, mbrman │
│ lazy_static       │     │ ureq, rustls, ring │
│ pc-keyboard       │     │ tempfile, serde    │
│ linked_list_alloc │     │ …                  │
│ lock_api          │     │                    │
│ …                │     │                    │
└───────────────────┘     └────────────────────┘
```

**Important**: The kernel and runner are **separate Cargo workspaces**. Crate_universe requires separate `from_cargo()` calls for each. On any `Cargo.lock` change, run `CARGO_BAZEL_REPIN=1 bazel build …` to regenerate.

### Platform constraints (`platforms/BUILD.bazel`)

Two Bazel platforms defined:
- `x86_64_bare_metal` — `@platforms//os:none` + `@platforms//cpu:x86_64`
- `x86_64_linux` — `@platforms//os:linux` + `@platforms//cpu:x86_64`

Used via `--config=bare` or `--config=host` in `.bazelrc`.

### Kernel targets (`kernel/BUILD.bazel`)

```
                      ┌───────────────────────────┐
                      │       yonti_os_lib        │
                      │  (rust_library, bare-     │
                      │   metal, crate_name=      │
                      │   "yonti_os")             │
                      │  srcs = lib.rs + all      │
                      │  modules                  │
                      └───────────┬───────────────┘
                                  │
              ┌───────────────────┼───────────────────┐
              │                   │                   │
              ▼                   ▼                   ▼
     ┌──────────────┐   ┌──────────────────┐  ┌───────────────┐
     │  yonti_os    │   │ yonti_os_lib_test│  │should_panic   │
     │ (binary,     │   │ (library, bare,  │  │  _elf         │
     │  main.rs →   │   │  --cfg test      │  │ (binary, bare,│
     │  kernel ELF) │   │  --cfg bazel)    │  │  depends on   │
     └──────────────┘   └────────┬─────────┘  │  yonti_os_lib)│
                                 │             └───────────────┘
                                 ▼
                        ┌───────────────┐
                        │ all_tests_elf │
                        │ (binary, bare,│
                        │  --test,      │
                        │  tests/all.rs │
                        │  + common/)   │
                        └───────────────┘
```

**`--cfg bazel` guard**: In `lib.rs`, the Cargo test harness (`entry_point!`, `test_kernel_main`, `panic_handler`) is gated behind `#[cfg(all(test, not(bazel)))]`. This allows Bazel to compile `yonti_os_lib_test` with `--cfg test` (enabling the public test API: `test_runner`, `QemuExitCode`, etc.) **without** also compiling the library's own entry point. The test binary (`all_tests_elf`) provides its own entry point in `tests/all.rs`.

### Convenience targets (`BUILD.bazel`)

| Target | Command | What it does |
|--------|---------|-------------|
| `//:fmt` | `bazel build //:fmt` | Runs `rustfmt` aspect over `yonti_os_lib` |
| `//:clippy` | `bazel build //:clippy` | Runs `clippy` aspect over `yonti_os_lib` |
| `//:deny` | `bazel run //:deny` | Runs `cargo-deny check` for both workspaces |

---

## Unified Test Binary

### Design

Tests are split into two categories:

| Category | Binary | Boots | Tests | Mechanism |
|----------|--------|-------|-------|-----------|
| **Unified** | `all_tests_elf` | 1 boot | 10 tests | Custom test framework, shared init |
| **Panic-expected** | `should_panic_elf` | 1 boot | 1 test | Standalone binary, `harness=false` |

**Before unification**: 4 separate test binaries (basic_boot, heap_allocation, file_system, should_panic) → 4 QEMU boots, ~93s total.

**After unification**: 2 binaries (all_tests_elf, should_panic_elf) → 2 QEMU boots, ~49s total (47% faster).

### How it works

```
tests/all.rs                              tests/common/
├── #![no_main]                           ├── basic_boot.rs     (test_println)
├── #![feature(custom_test_frameworks)]   ├── heap_allocation.rs (4 tests)
├── entry_point!(test_kernel_main, …)     └── file_system.rs    (5 tests)
├── fn test_kernel_main(...) {
│       yonti_os::init();           // PIC, IDT, SSE once
│       framebuffer::init(...);     // framebuffer once
│       BuddyAllocator::new(...);   // frame allocator once
│       init_heap(...);             // TLSF heap once
│       test_main();                // runs ALL #[test_case] fns
│   }
└── #[panic_handler]
```

The test entry point does **maximum initialization** (framebuffer + heap) once, then `test_main()` (generated by `custom_test_frameworks`) runs all 10 `#[test_case]` functions sequentially. The shared heap means all allocations (Box, Vec, file system data) coexist — tests use non-overlapping paths to isolate their data.

### `should_panic` remains standalone

`should_panic` cannot share a boot because it **expects a kernel panic** (it tests that `assert_eq!(0, 1)` causes the panic handler to fire). The panic handler exits QEMU with success code 33. Including this in the unified binary would terminate before other tests run.

---

## QEMU Test Infrastructure

### Flow

```
┌──────────────┐     ┌─────────────────┐     ┌──────────┐
│ Kernel ELF   │ ──→ │ DiskImageBuilder│ ──→ │ BIOS img │
│ (all_tests   │     │ (bootloader     │     │ (in tmp) │
│  _elf)       │     │  crate)         │     │          │
└──────────────┘     └─────────────────┘     └────┬─────┘
                                                  │
                                                  ▼
                                           ┌────────────┐
                                           │  QEMU      │
                                           │  -no-      │
                                           │  graphic   │
                                           │  -no-      │
                                           │  reboot    │
                                           │  -device   │
                                           │  isa-debug-│
                                           │  exit      │
                                           └──────┬─────┘
                                                  │
                                          exit code 33/35
                                                  │
                                                  ▼
                                          ┌──────────────┐
                                          │ test_runner  │
                                          │ maps: 33 → 0 │
                                          │       35 → 1 │
                                          └──────────────┘
```

### Current test execution

The `test-runner` binary (compiled by Cargo from `runner/src/test_runner.rs`) uses `DiskImageBuilder` from the `bootloader` crate to wrap a kernel ELF into a bootable BIOS disk image. It then spawns QEMU with `-device isa-debug-exit,iobase=0xf4,iosize=0x04` — the kernel writes 0x10 (success) or 0x11 (failure) to port 0xF4, which QEMU maps to exit codes 33 or 35.

**Transitional note**: The `bootloader` crate's build script (`build.rs`) compiles bootloader stages via Cargo internally, which doesn't work under Bazel yet. For now, Bazel builds kernel ELFs and the Cargo-built `test-runner` executes them. The Bazel-native `tools/qemu_runner.rs` is written but blocked on this limitation.

### Test runner script (`run_tests.sh`)

```sh
# Build kernel ELFs with Bazel
bazel build --config=bare //kernel:all_tests_elf //kernel:should_panic_elf

# Build test-runner with Cargo (one-time)
cargo build --no-default-features --bin test-runner

# Run each ELF in QEMU via test-runner
test-runner bazel-bin/kernel/all_tests_elf     # 10 tests, 1 boot
test-runner bazel-bin/kernel/should_panic_elf  # 1 test, 1 boot
```

---

## CI Pipeline (`.github/workflows/ci.yml`)

Triggered only on PRs to `master` (no duplicate run on merge). Branch protection requires all checks to pass.

Four jobs:

```
┌──────┐  ┌────────┐  ┌──────┐
│ fmt   │  │ clippy │  │ deny │   ← parallel, fast gates
└──┬───┘  └───┬─────┘  └──┬───┘
   │           │           │
   └───────────┼───────────┘
               │
      ┌────────▼─────────┐
      │  build-and-test  │   ← sequential, gated
      └──────────────────┘
```

| Job | Tech | What it checks |
|-----|------|---------------|
| `fmt` | Cargo | `cargo fmt --check` for kernel + runner |
| `clippy` | Cargo | `cargo clippy -- -D warnings` for both, kernel pre-built to warm runner cache |
| `deny` | cargo-deny | Advisories, licenses, bans for both workspaces |
| `build-and-test` | Cargo | `cargo build` kernel + runner + test ELFs, QEMU runs all tests |

### Caching

- **Cargo**: `~/.cargo/registry/`, `~/.cargo/git/`, `target/`, `runner/target/`
- **Keys**: `hashFiles('**/Cargo.lock')`

### Markdown-only PRs

PRs that change only `.md` files skip the full pipeline. A separate `markdown-lint.yml` workflow runs `markdownlint-cli2` instead.

### Branch protection

- `master` requires PR + all status checks to pass before merge
- CI runs only on `pull_request` — no duplicate run on merge commit

---

## Boot Process

```
QEMU starts
    │
    ▼
SeaBIOS → loads bootloader stage 1 (MBR)
    │
    ▼
Bootloader stages 2–4:
    • Sets up page tables, GDT, identity-maps physical memory
    • Parses kernel ELF, loads segments
    • Creates framebuffer mapping
    • Jumps to kernel entry point
    │
    ▼
kernel_main() in src/main.rs:
    1. yonti_os::init()        → SSE, IDT, PIC, enable interrupts
    2. framebuffer::init()     → pixel-based text renderer
    3. BuddyAllocator::new()   → physical frame allocator
    4. init_heap()             → map 257 pages, init TLSF heap
    5. demo_fs()               → in-memory filesystem demo
    6. Executor::run()         → async task executor
       ├── keyboard task       → prints keystrokes
       └── HLT idle            → sleep when no tasks ready
```

### Exit mechanism

The kernel writes a value to port `0xF4` (isa-debug-exit device). QEMU interprets this as:
- **0x10** → QEMU exit code 33 (test success)
- **0x11** → QEMU exit code 35 (test failure)

The test runner maps these to Unix exit codes 0 and 1.

---

## Memory Layout

```
Virtual address space:
  0x0000_0000_0000  ─┬─ Kernel code + data (loaded by bootloader)
                      │
  0x4444_4444_0000  ─┬─ HEAP_START
                      │   TLSF allocator, 1 MiB + 1 sentinel page
  0x4444_4454_1000  ─┘   (257 × 4 KiB pages mapped via buddy allocator)
                      │
  Physical memory:    │  Identity-mapped at bootloader-provided offset
  0x0000_0000_0000  ─┬─ Buddy allocator manages usable frames
                      │  MAX_ORDER = 10 (4 KiB to 4 MiB blocks)
```

---

## Dependency Graph (Kernel)

```
yonti_os_lib
├── bootloader_api 0.11.15   entry_point! macro, BootInfo, FrameBuffer
├── x86_64 0.15.4            Port I/O, paging (OffsetPageTable), IDT, GDT
├── spin 0.9.8               Mutex, Once (synchronization)
├── lazy_static 1.5.0        Static initialization (SERIAL1, PICS, FS, IDT)
├── pc-keyboard 0.5.1        Scancode → key event decoding
├── linked_list_allocator 0.10.2  Heap fallback (fixed_size_block.rs)
├── bit_field 0.10           (transitive, x86_64)
├── bitflags 2.11            (transitive, x86_64)
├── volatile 0.4             (transitive, x86_64)
├── lock_api 0.4             (transitive, spin)
├── scopeguard 1.2           (transitive, lock_api)
├── spinning_top 0.2         (transitive, linked_list_allocator)
├── const_fn 0.4             (proc-macro, build-time)
└── rustversion 1.0          (proc-macro, build-time)

Inline modules (no external deps):
├── kernel/src/uart.rs       (replaces uart_16550 crate, uses x86_64 Ports)
├── kernel/src/pic.rs        (replaces pic8259 crate, uses x86_64 Ports)
├── kernel/src/array_queue.rs (replaces crossbeam-queue, uses alloc + atomics)
├── kernel/src/async_utils.rs (replaces futures-util, uses core::task)
└── kernel/src/once_cell.rs   (replaces conquer-once, uses core::sync::atomic)

Total: 15 crates in Cargo.lock (down from 32 after dependency reduction)
```

---

## Quick Reference

| Operation | Cargo | Bazel (local dev) |
|-----------|-------|-------------------|
| Build kernel | `cd kernel && cargo build --target x86_64-unknown-none` | `bazel build --config=bare //kernel:yonti_os` |
| Build test ELFs | `cargo build --tests --target x86_64-unknown-none` | `bazel build --config=bare //kernel:all_tests_elf` |
| Run all tests | `./run_tests.sh` | `./run_tests.sh` |
| Format check | `cargo fmt --all -- --check` | `bazel build //:fmt` |
| Clippy check | `cargo clippy -- -D warnings` | `bazel build //:clippy` |
| Deny check | `cargo deny check` | `bazel run //:deny` |
| Repin deps | `cargo update` | `CARGO_BAZEL_REPIN=1 bazel build //kernel:yonti_os_lib` |
| Clean | `cargo clean` | `bazel clean` |
