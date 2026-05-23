//! Crash diagnostics and debugging utilities.
//!
//! Provides register dumps, stack backtraces (frame-pointer based),
//! and a full crash dump that combines info from the log, trace, and
//! monitor modules for post-mortem analysis.

use crate::log;
use x86_64::registers::control;

/// Read a general-purpose register via inline assembly.
macro_rules! read_reg {
    ($reg:ident) => {{
        let val: u64;
        unsafe {
            core::arch::asm!(concat!("mov {}, ", stringify!($reg)), out(reg) val, options(nomem, nostack, preserves_flags));
        }
        val
    }};
}

/// Print all general-purpose and control registers to serial.
pub fn dump_registers() {
    let rax = read_reg!(rax);
    let rbx = read_reg!(rbx);
    let rcx = read_reg!(rcx);
    let rdx = read_reg!(rdx);
    let rsi = read_reg!(rsi);
    let rdi = read_reg!(rdi);
    let rbp = read_reg!(rbp);
    let rsp = read_reg!(rsp);
    let r8 = read_reg!(r8);
    let r9 = read_reg!(r9);
    let r10 = read_reg!(r10);
    let r11 = read_reg!(r11);
    let r12 = read_reg!(r12);
    let r13 = read_reg!(r13);
    let r14 = read_reg!(r14);
    let r15 = read_reg!(r15);

    let cr0 = control::Cr0::read_raw();
    let cr2 = control::Cr2::read_raw();
    let cr3 = control::Cr3::read_raw().0.start_address().as_u64();
    let cr4 = control::Cr4::read_raw();

    log::error!("── REGISTERS ──────────────────────");
    log::error!(
        "RAX: {:#018x} RBX: {:#018x} RCX: {:#018x} RDX: {:#018x}",
        rax,
        rbx,
        rcx,
        rdx,
    );
    log::error!(
        "RSI: {:#018x} RDI: {:#018x} RBP: {:#018x} RSP: {:#018x}",
        rsi,
        rdi,
        rbp,
        rsp,
    );
    log::error!(
        "R8:  {:#018x} R9:  {:#018x} R10: {:#018x} R11: {:#018x}",
        r8,
        r9,
        r10,
        r11,
    );
    log::error!(
        "R12: {:#018x} R13: {:#018x} R14: {:#018x} R15: {:#018x}",
        r12,
        r13,
        r14,
        r15,
    );
    log::error!(
        "CR0: {:#010x} CR2: {:#018x} CR3: {:#018x} CR4: {:#010x}",
        cr0,
        cr2,
        cr3,
        cr4
    );
}

/// Print a stack backtrace using the frame pointer chain.
/// Requires the kernel to be compiled with `-C force-frame-pointers`.
/// Walks the RBP chain for up to 16 frames.
pub fn stack_trace() {
    let mut rbp = read_reg!(rbp);
    log::error!("── STACK TRACE ─────────────────────");
    for depth in 0..16u32 {
        if rbp == 0 || rbp < 0x1000 {
            break;
        }
        let return_addr = unsafe { *((rbp + 8) as *const u64) };
        let next_rbp = unsafe { *(rbp as *const u64) };
        log::error!("  {}: {:#018x}  (rbp={:#018x})", depth, return_addr, rbp);
        if next_rbp < rbp {
            break; // stack corruption
        }
        rbp = next_rbp;
    }
}

/// Full crash dump combining all diagnostic modules.
/// Called from the panic handler or double fault handler.
pub fn crash_dump(reason: &str, _file: &str, _line: u32) {
    log::error!("══════════ KERNEL CRASH ══════════");
    log::error!("reason: {}", reason);

    dump_registers();
    stack_trace();

    let metrics = crate::monitor::snapshot();
    crate::monitor::dump_to_serial(&metrics);
    crate::trace::dump_last(16);
    log::error!("════════════════════════════════════");
}

/// Print a hex dump of memory at the given address.
///
/// # Safety
///
/// The caller must ensure `addr` points to valid memory for `len` bytes.
pub unsafe fn hexdump(addr: *const u8, len: usize) {
    use core::fmt::Write;
    log::error!("── HEXDUMP {:#018x} ──", addr as usize);

    for offset in (0..len).step_by(16) {
        let mut line = alloc::string::String::new();
        let _ = write!(line, "  {:#010x}: ", addr as usize + offset);
        for i in 0..16 {
            if offset + i < len {
                let byte = unsafe { *addr.add(offset + i) };
                let _ = write!(line, "{:02x} ", byte);
            } else {
                let _ = write!(line, "   ");
            }
        }
        log::error!("{}", line);
    }
}
