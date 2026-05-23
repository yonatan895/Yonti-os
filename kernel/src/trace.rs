//! Lock-free execution tracing ring buffer.
//!
//! Records structured trace events in a fixed-size ring buffer.
//! On kernel panic, the buffer can be dumped to serial for diagnostics.
//! SPSC design: writers produce from any context, reader consumes from main context.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const BUFFER_SIZE: usize = 4096;

// ── Event ID registry ────────────────────────────────────────────

#[repr(u16)]
pub enum TraceEventId {
    Alloc = 1,
    Free = 2,
    TaskSpawn = 10,
    TaskComplete = 11,
    Interrupt = 20,
    PageFault = 30,
}

// ── Ring buffer ──────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct TraceEntry {
    timestamp: u64,
    event_id: u16,
    arg0: u64,
    arg1: u64,
}

struct TraceBuffer {
    entries: UnsafeCell<MaybeUninit<[TraceEntry; BUFFER_SIZE]>>,
    write_idx: AtomicUsize,
    overflow_count: AtomicU64,
    initialized: AtomicU64,
}

// SAFETY: single-core, writers in ISR context, reader in main context.
unsafe impl Sync for TraceBuffer {}

static TRACE: TraceBuffer = TraceBuffer {
    entries: UnsafeCell::new(MaybeUninit::zeroed()),
    write_idx: AtomicUsize::new(0),
    overflow_count: AtomicU64::new(0),
    initialized: AtomicU64::new(0),
};

/// Initialize the trace buffer. Call once during boot.
pub fn init() {
    TRACE.initialized.store(1, Ordering::Release);
}

#[inline(always)]
fn is_init() -> bool {
    TRACE.initialized.load(Ordering::Acquire) != 0
}

/// Record an event. Safe to call from interrupt context.
/// No-op if the trace buffer hasn't been initialized.
pub fn record(id: TraceEventId, arg0: u64, arg1: u64) {
    if !is_init() {
        return;
    }
    x86_64::instructions::interrupts::without_interrupts(|| {
        let idx = TRACE.write_idx.fetch_add(1, Ordering::Relaxed) % BUFFER_SIZE;
        // Detect overflow (statistical — not exact)
        if idx == 0 {
            TRACE.overflow_count.fetch_add(1, Ordering::Relaxed);
        }
        unsafe {
            let entries: &mut [TraceEntry; BUFFER_SIZE] = (*TRACE.entries.get()).assume_init_mut();
            entries[idx] = TraceEntry {
                timestamp: read_tsc(),
                event_id: id as u16,
                arg0,
                arg1,
            };
        }
    });
}

/// Shorthand for recording an event with no arguments
#[macro_export]
macro_rules! trace_event {
    ($id:expr) => {
        $crate::trace::record($id, 0, 0)
    };
    ($id:expr, $a0:expr) => {
        $crate::trace::record($id, $a0 as u64, 0)
    };
    ($id:expr, $a0:expr, $a1:expr) => {
        $crate::trace::record($id, $a0 as u64, $a1 as u64)
    };
}

/// Dump the last N events to serial via the log crate.
pub fn dump_last(n: usize) {
    use crate::log::info;

    x86_64::instructions::interrupts::without_interrupts(|| {
        let total = TRACE.write_idx.load(Ordering::Relaxed);
        let start = total.saturating_sub(n);

        info!(
            "trace events: total={} overflow={} (showing last {})",
            total,
            TRACE.overflow_count.load(Ordering::Relaxed),
            n,
        );

        for i in start..total {
            let entry = unsafe {
                let entries: &[TraceEntry; BUFFER_SIZE] = (*TRACE.entries.get()).assume_init_ref();
                &entries[i % BUFFER_SIZE]
            };
            let name = match entry.event_id {
                1 => "ALLOC",
                2 => "FREE",
                10 => "TASK_SPAWN",
                11 => "TASK_COMPLETE",
                20 => "IRQ",
                30 => "PAGE_FAULT",
                _ => "UNKNOWN",
            };
            info!(
                "  [{:016x}] {} arg0={:#x} arg1={:#x}",
                entry.timestamp, name, entry.arg0, entry.arg1,
            );
        }
    });
}

fn read_tsc() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") low, out("edx") high, options(nomem, nostack));
    }
    ((high as u64) << 32) | (low as u64)
}
