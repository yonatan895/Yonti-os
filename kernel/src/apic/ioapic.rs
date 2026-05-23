//! I/O APIC MMIO driver.
//!
//! The I/O APIC is memory-mapped at a physical address discovered via
//! the MADT ACPI table (typically `0xFEC0_0000`). It uses indirect
//! register access: write the register selector to IOREGSEL (offset 0),
//! then read/write the data at IOWIN (offset 0x10).
//!
//! Reference: Intel 82093AA I/O APIC datasheet.

use core::ptr;

/// Register select offset.
const IOREGSEL: usize = 0x00;

/// Window / data offset.
const IOWIN: usize = 0x10;

// IO APIC identification register (index 0).
// const IOAPICID: u8 = 0x00;

/// IO APIC version register (index 1).
const IOAPICVER: u8 = 0x01;

/// First redirection table register (indices 0x10–0x3F for 24 entries).
const IOREDTBL_BASE: u8 = 0x10;

/// Redirection entry flags for edge-triggered, active-high, physical
/// destination mode, fixed delivery.
const REDIR_FLAGS: u32 = 0;

/// Bit to mask (disable) a redirection entry.
const REDIR_MASKED: u32 = 1 << 16;

pub struct IoApic {
    base: *mut u32,
    gsi_base: u32,
    max_redirs: u8,
}

impl IoApic {
    /// Create a new I/O APIC wrapper from a **virtual** base address.
    ///
    /// `gsi_base` is the starting Global System Interrupt number that
    /// this I/O APIC's input pins correspond to (from the MADT).
    ///
    /// # Safety
    ///
    /// The caller must ensure `base` points to a properly mapped
    /// I/O APIC MMIO region.
    pub unsafe fn new(base: *mut u32, gsi_base: u32) -> Self {
        assert!(!base.is_null(), "I/O APIC base pointer must not be null");

        let version = unsafe { Self::read_reg(base, IOAPICVER) };
        let max_redirs = ((version >> 16) & 0xFF) as u8;
        assert!(max_redirs > 0, "I/O APIC has no redirection entries");
        Self {
            base,
            gsi_base,
            max_redirs,
        }
    }

    /// Read a 32-bit register by indirect access.
    unsafe fn read_reg(base: *mut u32, index: u8) -> u32 {
        unsafe {
            ptr::write_volatile(base.byte_add(IOREGSEL).cast::<u32>(), index as u32);
            ptr::read_volatile(base.byte_add(IOWIN).cast::<u32>())
        }
    }

    /// Write a 32-bit register by indirect access.
    unsafe fn write_reg(base: *mut u32, index: u8, value: u32) {
        unsafe {
            ptr::write_volatile(base.byte_add(IOREGSEL).cast::<u32>(), index as u32);
            ptr::write_volatile(base.byte_add(IOWIN).cast::<u32>(), value);
        }
    }

    fn read(&self, index: u8) -> u32 {
        unsafe { Self::read_reg(self.base, index) }
    }

    fn write(&self, index: u8, value: u32) {
        unsafe {
            Self::write_reg(self.base, index, value);
        }
    }

    /// Map an IRQ source (input pin 0..max) to a destination IDT vector.
    ///
    /// `irq` is the pin number local to this I/O APIC (IRQ# - gsi_base).
    /// `vector` is the 8-bit IDT vector to deliver.
    ///
    /// # Safety
    ///
    /// Must only be called during initialization, before interrupts are
    /// enabled. The vector must have a valid handler installed in the IDT.
    pub unsafe fn set_irq(&self, irq: u8, vector: u8) {
        assert!(irq <= self.max_redirs);
        assert!(
            vector >= 32,
            "IOAPIC vector {} would conflict with CPU exceptions",
            vector
        );
        let idx = IOREDTBL_BASE + irq * 2;
        let lo = (vector as u32) | REDIR_FLAGS;
        let hi = 0u32;

        unsafe {
            Self::write_reg(self.base, idx, lo);
            Self::write_reg(self.base, idx + 1, hi);
        }
    }

    /// Mask (disable) a single redirection entry.
    ///
    /// # Safety
    ///
    /// The I/O APIC MMIO region must still be mapped.
    pub unsafe fn mask_irq(&self, irq: u8) {
        assert!(irq <= self.max_redirs);
        let idx = IOREDTBL_BASE + irq * 2;
        let mut lo = self.read(idx);
        lo |= REDIR_MASKED;
        self.write(idx, lo);
    }

    /// Return the GSI base for this I/O APIC.
    pub fn gsi_base(&self) -> u32 {
        self.gsi_base
    }
}
