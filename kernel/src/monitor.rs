//! Runtime metrics and counters for kernel observability.
//!
//! Lock-free (single-core) atomic counters for allocation, task,
//! interrupt, and timing statistics. Counters are updated from
//! anywhere (including ISR context) and can be dumped to serial
//! as a JSON snapshot.

use crate::log;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// ── Metric storage ────────────────────────────────────────────────

static MONITOR: MonitorData = MonitorData::new();

struct MonitorData {
    // Heap (updated by global allocator)
    alloc_count: AtomicU64,
    free_count: AtomicU64,
    current_allocated: AtomicUsize,
    peak_allocated: AtomicUsize,

    // Physical frames (updated by buddy allocator)
    allocated_frames: AtomicUsize,
    total_frames: AtomicUsize,

    // Tasks (updated by executor)
    tasks_spawned: AtomicU64,
    tasks_completed: AtomicU64,
    active_tasks: AtomicUsize,

    // Interrupts — one slot per PIC IRQ (0x20..0x2F = 16 irqs)
    interrupt_counts: [AtomicU64; 16],

    // Timer (updated by timer ISR)
    timer_ticks: AtomicU64,

    // Executor (wake drops)
    dropped_wakes: AtomicU64,
}

impl MonitorData {
    const fn new() -> Self {
        Self {
            alloc_count: AtomicU64::new(0),
            free_count: AtomicU64::new(0),
            current_allocated: AtomicUsize::new(0),
            peak_allocated: AtomicUsize::new(0),
            allocated_frames: AtomicUsize::new(0),
            total_frames: AtomicUsize::new(0),
            tasks_spawned: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            active_tasks: AtomicUsize::new(0),
            interrupt_counts: [const { AtomicU64::new(0) }; 16],
            timer_ticks: AtomicU64::new(0),
            dropped_wakes: AtomicU64::new(0),
        }
    }
}

// ── Heap metrics ──────────────────────────────────────────────────

pub fn inc_alloc(size: usize) {
    MONITOR.alloc_count.fetch_add(1, Ordering::Relaxed);
    let prev = MONITOR.current_allocated.fetch_add(size, Ordering::Relaxed);
    let new = prev + size;
    // Track peak: only CAS if we set a new record
    let mut peak = MONITOR.peak_allocated.load(Ordering::Relaxed);
    while new > peak {
        match MONITOR.peak_allocated.compare_exchange_weak(
            peak,
            new,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => peak = actual,
        }
    }
}

pub fn inc_free(size: usize) {
    MONITOR.free_count.fetch_add(1, Ordering::Relaxed);
    MONITOR.current_allocated.fetch_sub(size, Ordering::Relaxed);
}

// ── Frame metrics ─────────────────────────────────────────────────

pub fn set_frame_metrics(total: usize) {
    MONITOR.total_frames.store(total, Ordering::Relaxed);
}

pub fn inc_allocated_frames(count: usize) {
    MONITOR.allocated_frames.fetch_add(count, Ordering::Relaxed);
}

pub fn dec_allocated_frames(count: usize) {
    MONITOR.allocated_frames.fetch_sub(count, Ordering::Relaxed);
}

// ── Task metrics ──────────────────────────────────────────────────

pub fn inc_task_spawned() {
    MONITOR.tasks_spawned.fetch_add(1, Ordering::Relaxed);
    MONITOR.active_tasks.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_task_completed() {
    MONITOR.tasks_completed.fetch_add(1, Ordering::Relaxed);
    MONITOR.active_tasks.fetch_sub(1, Ordering::Relaxed);
}

// ── Interrupt metrics ─────────────────────────────────────────────

pub fn inc_interrupt(irq: u8) {
    let idx = irq.saturating_sub(0x20) as usize;
    if idx < 16 {
        MONITOR.interrupt_counts[idx].fetch_add(1, Ordering::Relaxed);
    }
}

// ── Timer ─────────────────────────────────────────────────────────

pub fn inc_timer_tick() {
    MONITOR.timer_ticks.fetch_add(1, Ordering::Relaxed);
}

pub fn uptime_ticks() -> u64 {
    MONITOR.timer_ticks.load(Ordering::Relaxed)
}

// ── Executor ───────────────────────────────────────────────────────

pub fn inc_dropped_wake() {
    MONITOR.dropped_wakes.fetch_add(1, Ordering::Relaxed);
}

// ── Snapshot ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct MetricsSnapshot {
    pub alloc_count: u64,
    pub free_count: u64,
    pub current_allocated: usize,
    pub peak_allocated: usize,
    pub allocated_frames: usize,
    pub total_frames: usize,
    pub tasks_spawned: u64,
    pub tasks_completed: u64,
    pub active_tasks: usize,
    pub interrupt_counts: [u64; 16],
    pub timer_ticks: u64,
    pub dropped_wakes: u64,
}

pub fn snapshot() -> MetricsSnapshot {
    let mut irq_counts = [0u64; 16];
    for (i, count) in irq_counts.iter_mut().enumerate() {
        *count = MONITOR.interrupt_counts[i].load(Ordering::Relaxed);
    }

    MetricsSnapshot {
        alloc_count: MONITOR.alloc_count.load(Ordering::Relaxed),
        free_count: MONITOR.free_count.load(Ordering::Relaxed),
        current_allocated: MONITOR.current_allocated.load(Ordering::Relaxed),
        peak_allocated: MONITOR.peak_allocated.load(Ordering::Relaxed),
        allocated_frames: MONITOR.allocated_frames.load(Ordering::Relaxed),
        total_frames: MONITOR.total_frames.load(Ordering::Relaxed),
        tasks_spawned: MONITOR.tasks_spawned.load(Ordering::Relaxed),
        tasks_completed: MONITOR.tasks_completed.load(Ordering::Relaxed),
        active_tasks: MONITOR.active_tasks.load(Ordering::Relaxed),
        interrupt_counts: irq_counts,
        timer_ticks: MONITOR.timer_ticks.load(Ordering::Relaxed),
        dropped_wakes: MONITOR.dropped_wakes.load(Ordering::Relaxed),
    }
}

pub fn dump_to_serial(metrics: &MetricsSnapshot) {
    log::info!(
        "metrics: alloc={} free={} cur={} peak={} frames={}/{} tasks={}/{}/{} drops={} ticks={}",
        metrics.alloc_count,
        metrics.free_count,
        metrics.current_allocated,
        metrics.peak_allocated,
        metrics.allocated_frames,
        metrics.total_frames,
        metrics.tasks_spawned,
        metrics.tasks_completed,
        metrics.active_tasks,
        metrics.dropped_wakes,
        metrics.timer_ticks,
    );

    // Interrupt distribution
    for (i, &count) in metrics.interrupt_counts.iter().enumerate() {
        if count > 0 {
            log::info!("  irq 0x{:02x}: {}", 0x20 + i as u8, count);
        }
    }
}
