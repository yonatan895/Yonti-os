//! Structured kernel logging via the `log` crate facade.
//!
//! Provides leveled macros (`error!`, `warn!`, `info!`, `debug!`, `trace!`)
//! that write to the serial port with the format `[LEVEL] module: message`.
//! The existing `println!` and `serial_println!` macros continue to work
//! independently for framebuffer and serial-only output.
//!
//! # Usage
//!
//! ```ignore
//! use crate::log::info;
//!
//! info!("heap initialized: {} bytes", HEAP_SIZE);
//! warn!("scancode queue full, dropping input");
//! error!("page fault at {:#x}", addr);
//! debug!("mapping page {} to frame {}", p, f);  // stripped in release
//! ```
//!
//! # Compile-time filtering
//!
//! In release builds (`--release`), `debug!` and `trace!` are compile-time
//! no-ops (zero cost). Use `--features log_debug` or `--features log_trace`
//! to enable them in release builds. `info!`, `warn!`, and `error!` are
//! always available.

use crate::serial;
use log::{Log, Metadata, Record, SetLoggerError};

/// Singleton logger. Registered once during kernel boot.
static LOGGER: KernelLogger = KernelLogger;

struct KernelLogger;

impl Log for KernelLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // Output format: [LEVEL] module_path: message\n
        serial::_print(format_args!(
            "[{:<5}] {}: {}\n",
            record.level(),
            record.target(),
            record.args(),
        ));
    }

    fn flush(&self) {}
}

/// Initialize the global logger. Must be called once during kernel boot,
/// after the serial port is configured.
///
/// # Panics
///
/// Panics if called more than once (logger has already been set).
pub fn init(level: LevelFilter) -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(level);
    Ok(())
}

// Re-export the `log` crate macros and types so kernel code can import from `crate::log`.
pub use log::LevelFilter;
pub use log::{debug, error, info, trace, warn};
