//! Minimal 8259 PIC driver.
//!
//! Replaces the `pic8259` crate to eliminate the duplicate `x86_64 0.14.x`
//! transitive dependency. Uses the kernel's own `x86_64 0.15.x` Port types.

use x86_64::instructions::port::Port;

/// Command sent to begin PIC initialization.
const CMD_INIT: u8 = 0x11;

/// Command sent to acknowledge an interrupt.
const CMD_END_OF_INTERRUPT: u8 = 0x20;

/// 8086/88 (MCS-80/85) mode.
const MODE_8086: u8 = 0x01;

struct Pic {
    offset: u8,
    command: Port<u8>,
    data: Port<u8>,
}

impl Pic {
    fn handles_interrupt(&self, interrupt_id: u8) -> bool {
        self.offset <= interrupt_id && interrupt_id < self.offset + 8
    }

    unsafe fn end_of_interrupt(&mut self) {
        unsafe {
            self.command.write(CMD_END_OF_INTERRUPT);
        }
    }
}

pub struct ChainedPics {
    pics: [Pic; 2],
}

impl ChainedPics {
    /// Create a new chained PIC interface with the given interrupt offsets.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the PICs are present and not already
    /// initialized by another driver.
    pub const unsafe fn new(offset1: u8, offset2: u8) -> Self {
        Self {
            pics: [
                Pic {
                    offset: offset1,
                    command: Port::new(0x20),
                    data: Port::new(0x21),
                },
                Pic {
                    offset: offset2,
                    command: Port::new(0xA0),
                    data: Port::new(0xA1),
                },
            ],
        }
    }

    /// Initialize both PIC controllers with ICW1–ICW4.
    ///
    /// # Safety
    ///
    /// Must be called only once and before enabling interrupts.
    pub unsafe fn initialize(&mut self) {
        unsafe {
            let mut wait_port: Port<u8> = Port::new(0x80);
            let mut wait = || wait_port.write(0);

            // Save masks
            let saved_mask1 = self.pics[0].data.read();
            let saved_mask2 = self.pics[1].data.read();

            // ICW1: start initialization
            self.pics[0].command.write(CMD_INIT);
            wait();
            self.pics[1].command.write(CMD_INIT);
            wait();

            // ICW2: base offsets
            self.pics[0].data.write(self.pics[0].offset);
            wait();
            self.pics[1].data.write(self.pics[1].offset);
            wait();

            // ICW3: chaining (PIC1: slave on IR2, PIC2: cascade identity)
            self.pics[0].data.write(4);
            wait();
            self.pics[1].data.write(2);
            wait();

            // ICW4: 8086 mode
            self.pics[0].data.write(MODE_8086);
            wait();
            self.pics[1].data.write(MODE_8086);
            wait();

            // Restore masks
            self.pics[0].data.write(saved_mask1);
            self.pics[1].data.write(saved_mask2);
        }
    }

    /// Notify the PIC(s) that an interrupt has been handled.
    ///
    /// # Safety
    ///
    /// Must be called from the interrupt handler for the given `interrupt_id`.
    pub unsafe fn notify_end_of_interrupt(&mut self, interrupt_id: u8) {
        unsafe {
            if self.pics[1].handles_interrupt(interrupt_id) {
                self.pics[1].end_of_interrupt();
            }
            self.pics[0].end_of_interrupt();
        }
    }

    /// Mask (disable) a single IRQ line on the PIC.
    ///
    /// `irq` is the hardware IRQ number (0–15).
    ///
    /// # Safety
    ///
    /// Must be called with interrupts disabled.
    pub unsafe fn mask_irq(&mut self, irq: u8) {
        assert!(irq < 16);
        if irq < 8 {
            let mut mask = unsafe { self.pics[0].data.read() };
            mask |= 1 << irq;
            unsafe {
                self.pics[0].data.write(mask);
            }
        } else {
            let mut mask = unsafe { self.pics[1].data.read() };
            mask |= 1 << (irq - 8);
            unsafe {
                self.pics[1].data.write(mask);
            }
        }
    }

    /// Mask all IRQ lines so the PIC no longer forwards interrupts.
    ///
    /// # Safety
    ///
    /// Must be called with interrupts disabled and after the APIC has
    /// been configured to take over interrupt routing.
    pub unsafe fn mask_all(&mut self) {
        unsafe {
            self.pics[0].data.write(0xFF);
            self.pics[1].data.write(0xFF);
        }
    }
}
