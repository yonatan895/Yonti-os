//! Integration tests for the APIC subsystem.
//!
//! These run inside QEMU with full hardware access and verify that the
//! APIC is detected, initialised, and correctly delivering interrupts.

use yonti_os::apic;
use yonti_os::monitor;
use yonti_os::serial_println;

/// APIC must be active after boot.
#[test_case]
fn apic_01_active_after_boot() {
    assert!(
        apic::is_active(),
        "APIC must be active after successful detection + init"
    );
}

/// Timer ticks must advance, proving that the PIT → I/O APIC → LAPIC
/// interrupt path is functional with correct EOI handling.
///
/// The PIT typically runs at ~18.2 Hz (one tick every ~55 ms).  We spin
/// long enough to guarantee at least one tick fires.
#[test_case]
fn apic_02_timer_ticks_advance() {
    let t0 = monitor::uptime_ticks();

    // Spin for roughly 120 ms — more than two PIT periods at 18.2 Hz.
    // This ensures at least one tick fires even at the slowest rate.
    for _ in 0..12_000_000 {
        core::hint::spin_loop();
    }

    let t1 = monitor::uptime_ticks();
    let delta = t1.wrapping_sub(t0);

    serial_println!("timer ticks: t0={} t1={} delta={}", t0, t1, delta);

    assert!(
        delta > 0,
        "no timer ticks in 120 ms — APIC interrupt delivery is broken"
    );
}

/// Interrupt counters from the monitor should reflect that both the
/// timer and keyboard vectors are receiving interrupts through APIC.
///
/// By the time this test runs, several timer ticks have been delivered.
#[test_case]
fn apic_03_interrupt_counts_nonzero() {
    let snap = monitor::snapshot();
    let timer_count = snap.interrupt_counts[0]; // IRQ 0 → vector 32
    let kbd_count = snap.interrupt_counts[1]; // IRQ 1 → vector 33

    serial_println!("interrupt counts: timer={} kbd={}", timer_count, kbd_count);

    assert!(
        timer_count > 0,
        "timer interrupt count is 0 — APIC not delivering IRQ 0",
    );
    // Keyboard count may be 0 if no keys were pressed; we don't assert it.
    let _ = kbd_count;
}

/// Timer ticks are monotonic across multiple readings.
#[test_case]
fn apic_04_ticks_monotonic() {
    for _ in 0..10 {
        let a = monitor::uptime_ticks();
        let b = monitor::uptime_ticks();
        assert!(b >= a, "timer ticks went backwards: {} → {}", a, b);
    }
}

/// The fact that we reached this test without a triple-fault proves
/// the APIC EOI path is functional.  A missing or incorrect EOI would
/// freeze the system at the first timer IRQ.
#[test_case]
fn apic_05_eoi_no_triple_fault() {
    // Existence is the proof — this line only executes if interrupts
    // have been firing and EOI has been sent correctly.
    assert!(apic::is_active());
}

// ---------------------------------------------------------------------------
// Unit tests for pure functions (no hardware needed)
// ---------------------------------------------------------------------------

#[test_case]
fn apic_unit_build_irq_gsi_map_identity() {
    let overrides = [];
    let map = apic::build_irq_gsi_map(&overrides, 0);
    for i in 0..16 {
        assert_eq!(map[i], i as u32);
    }
}

#[test_case]
fn apic_unit_build_irq_gsi_map_single_override() {
    use yonti_os::apic::IrqOverride;
    let overrides = [IrqOverride {
        source: 0,
        gsi: 2,
        flags: 0,
    }];
    let map = apic::build_irq_gsi_map(&overrides, 1);
    assert_eq!(map[0], 2);
    assert_eq!(map[1], 1);
    assert_eq!(map[2], 2);
}

#[test_case]
fn apic_unit_build_irq_gsi_map_multiple_overrides() {
    use yonti_os::apic::IrqOverride;
    let overrides = [
        IrqOverride {
            source: 0,
            gsi: 2,
            flags: 0,
        },
        IrqOverride {
            source: 9,
            gsi: 21,
            flags: 0,
        },
    ];
    let map = apic::build_irq_gsi_map(&overrides, 2);
    assert_eq!(map[0], 2);
    assert_eq!(map[9], 21);
    assert_eq!(map[1], 1);
    assert_eq!(map[15], 15);
}

#[test_case]
fn apic_unit_build_irq_gsi_map_override_out_of_range() {
    use yonti_os::apic::IrqOverride;
    let overrides = [IrqOverride {
        source: 20,
        gsi: 50,
        flags: 0,
    }];
    let map = apic::build_irq_gsi_map(&overrides, 1);
    for i in 0..16 {
        assert_eq!(map[i], i as u32);
    }
}

#[test_case]
fn apic_unit_build_irq_gsi_map_empty() {
    use yonti_os::apic::IrqOverride;
    let overrides = [IrqOverride {
        source: 0,
        gsi: 0,
        flags: 0,
    }; 16];
    let map = apic::build_irq_gsi_map(&overrides, 0);
    for i in 0..16 {
        assert_eq!(map[i], i as u32);
    }
}

#[test_case]
fn apic_unit_check_signature_match() {
    let buf: [u8; 4] = *b"APIC";
    assert!(apic::check_signature(buf.as_ptr(), b"APIC"));
    assert!(!apic::check_signature(buf.as_ptr(), b"RSDT"));
}

#[test_case]
fn apic_unit_check_signature_null() {
    assert!(!apic::check_signature(core::ptr::null(), b"APIC"));
}
