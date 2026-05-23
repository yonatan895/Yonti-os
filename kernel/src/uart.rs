//! Minimal port-mapped UART 16550 driver.
//!
//! Replaces the `uart_16550` crate to eliminate the duplicate `x86_64 0.14.x`
//! transitive dependency. Uses the kernel's own `x86_64 0.15.x` Port types.

use core::fmt;
use x86_64::instructions::port::{Port, PortReadOnly, PortWriteOnly};

// Line Status Register flags

const LINE_STS_OUTPUT_EMPTY: u8 = 1 << 5;

macro_rules! wait_for {
    ($cond:expr) => {
        while !$cond {
            core::hint::spin_loop()
        }
    };
}

pub struct SerialPort {
    data: Port<u8>,
    int_en: PortWriteOnly<u8>,
    fifo_ctrl: PortWriteOnly<u8>,
    line_ctrl: PortWriteOnly<u8>,
    modem_ctrl: PortWriteOnly<u8>,
    line_sts: PortReadOnly<u8>,
}

impl SerialPort {
    /// Create a new serial port at the given I/O base address.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the given base address points to a
    /// valid UART 16550 device.
    pub const unsafe fn new(base: u16) -> Self {
        Self {
            data: Port::new(base),
            int_en: PortWriteOnly::new(base + 1),
            fifo_ctrl: PortWriteOnly::new(base + 2),
            line_ctrl: PortWriteOnly::new(base + 3),
            modem_ctrl: PortWriteOnly::new(base + 4),
            line_sts: PortReadOnly::new(base + 5),
        }
    }

    /// Initializes the serial port with 38400/8-N-1.
    pub fn init(&mut self) {
        unsafe {
            // Disable interrupts
            self.int_en.write(0x00);

            // Enable DLAB (divisor latch access bit)
            self.line_ctrl.write(0x80);

            // Set baud divisor to 38400 bps (115200 / 3 = 38400)
            self.data.write(0x03); // DLL low byte
            self.int_en.write(0x00); // DLM high byte

            // Disable DLAB, set 8 data bits, no parity, 1 stop bit
            self.line_ctrl.write(0x03);

            // Enable FIFO, clear TX/RX queues, interrupt watermark at 14 bytes
            self.fifo_ctrl.write(0xC7);

            // RTS/DSR and auxiliary output #2 (interrupt line)
            self.modem_ctrl.write(0x0B);

            // Enable interrupts
            self.int_en.write(0x01);
        }
    }

    fn line_sts_flags(&mut self) -> u8 {
        unsafe { self.line_sts.read() }
    }

    pub fn send(&mut self, data: u8) {
        unsafe {
            match data {
                8 | 0x7F => {
                    // Backspace or DEL: send BS-space-BS to erase
                    wait_for!(self.line_sts_flags() & LINE_STS_OUTPUT_EMPTY != 0);
                    self.data.write(8);
                    wait_for!(self.line_sts_flags() & LINE_STS_OUTPUT_EMPTY != 0);
                    self.data.write(b' ');
                    wait_for!(self.line_sts_flags() & LINE_STS_OUTPUT_EMPTY != 0);
                    self.data.write(8);
                }
                _ => {
                    wait_for!(self.line_sts_flags() & LINE_STS_OUTPUT_EMPTY != 0);
                    self.data.write(data);
                }
            }
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.send(byte);
        }
        Ok(())
    }
}
