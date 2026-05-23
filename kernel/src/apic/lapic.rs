//! Local APIC (LAPIC) MMIO driver.
//!
//! The LAPIC is memory-mapped at a base physical address (default
//! `0xFEE0_0000`). All register accesses use 32-bit `read_volatile` /
//! `write_volatile`.
//!
//! References: Intel SDM Vol. 3, Chapter 10.

use core::ptr;

/// APIC ID Register (R).
const LAPIC_ID: usize = 0x020;

/// APIC Version Register (R).
const LAPIC_VERSION: usize = 0x030;

// Task Priority Register (R/W).
// const LAPIC_TPR: usize = 0x080;

/// End-of-Interrupt Register (W).
pub const LAPIC_EOI: usize = 0x0B0;

/// Spurious Interrupt Vector Register (R/W).
const LAPIC_SVR: usize = 0x0F0;

// Interrupt Command Register low (R/W).
// const LAPIC_ICR_LO: usize = 0x300;

// Interrupt Command Register high (R/W).
// const LAPIC_ICR_HI: usize = 0x310;

/// Spurious Interrupt Vector Register: enable bit (bit 8).
const SVR_ENABLE: u32 = 1 << 8;

/// The physical base address of the LAPIC on most x86_64 systems.
pub const DEFAULT_PHYS_BASE: u64 = 0xFEE0_0000;

pub struct Lapic {
    base: *mut u32,
}

unsafe impl Send for Lapic {}
unsafe impl Sync for Lapic {}

impl Lapic {
    /// Create a new LAPIC wrapper from a **virtual** base address.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `base` points to a properly mapped
    /// LAPIC MMIO region (typically `phys_mem_offset + 0xFEE0_0000`).
    pub unsafe fn new(base: *mut u32) -> Self {
        assert!(!base.is_null(), "LAPIC base pointer must not be null");
        Self { base }
    }

    fn read(&self, offset_bytes: usize) -> u32 {
        assert!(offset_bytes.is_multiple_of(4));
        unsafe { ptr::read_volatile(self.base.byte_add(offset_bytes).cast::<u32>()) }
    }

    fn write(&self, offset_bytes: usize, value: u32) {
        assert!(offset_bytes.is_multiple_of(4));
        unsafe { ptr::write_volatile(self.base.byte_add(offset_bytes).cast::<u32>(), value) }
    }

    pub fn id(&self) -> u32 {
        let id = (self.read(LAPIC_ID) >> 24) & 0xFF;
        assert!(id < 16, "LAPIC ID out of expected range (0-15)");
        id
    }

    pub fn version(&self) -> u32 {
        let v = self.read(LAPIC_VERSION) & 0xFF;
        assert!(v > 0, "LAPIC version is zero — LAPIC not present?");
        v
    }

    /// Enable the local APIC via the Spurious Interrupt Vector Register.
    ///
    /// `spurious_vector` is the vector delivered when a spurious interrupt
    /// occurs (bits 0–7). The caller should choose a vector unlikely to
    /// collide with real interrupt handlers (e.g. `0xFF`).
    ///
    /// # Safety
    ///
    /// Must be called before any APIC interrupts are expected. The MMIO
    /// region must remain mapped.
    pub unsafe fn enable(&self, spurious_vector: u8) {
        let value = (spurious_vector as u32) | SVR_ENABLE;
        self.write(LAPIC_SVR, value);

        let svr = self.read(LAPIC_SVR);
        assert!(
            svr & SVR_ENABLE != 0,
            "LAPIC enable failed: SVR=0x{:x}",
            svr
        );
    }

    /// Signal end-of-interrupt.
    ///
    /// Must be called by every interrupt handler that uses APIC routing.
    /// Failing to do so blocks all lower-or-equal priority interrupts.
    ///
    /// # Safety
    ///
    /// Must only be called when the LAPIC is active and the MMIO region
    /// is still mapped.
    pub unsafe fn end_of_interrupt(&self) {
        self.write(LAPIC_EOI, 0);
    }
}
