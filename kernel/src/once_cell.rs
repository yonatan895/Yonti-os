//! Minimal `OnceCell` for single-threaded kernel use.
//!
//! Replaces `conquer-once`'s `OnceCell` with a simpler implementation
//! that uses `AtomicBool` + `UnsafeCell` instead of a full CAS-based
//! state machine. Correct for single-core cooperative scheduling.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

pub struct OnceCell<T> {
    initialized: AtomicBool,
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> OnceCell<T> {
    pub const fn uninit() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub fn try_get(&self) -> Result<&T, TryGetError> {
        if self.initialized.load(Ordering::Acquire) {
            Ok(unsafe { &*(*self.value.get()).as_ptr() })
        } else {
            Err(TryGetError::Uninit)
        }
    }

    pub fn try_init_once(&self, f: impl FnOnce() -> T) -> Result<(), TryInitError> {
        x86_64::instructions::interrupts::without_interrupts(|| {
            if self.initialized.load(Ordering::Acquire) {
                return Err(TryInitError::AlreadyInit);
            }

            let val = f();
            unsafe {
                (*self.value.get()).write(val);
            }
            self.initialized.store(true, Ordering::Release);
            Ok(())
        })
    }
}

unsafe impl<T: Send> Send for OnceCell<T> {}
unsafe impl<T: Sync> Sync for OnceCell<T> {}

#[derive(Debug)]
pub enum TryGetError {
    Uninit,
}

#[derive(Debug)]
pub enum TryInitError {
    AlreadyInit,
}
