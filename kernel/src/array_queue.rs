//! Minimal lock-free bounded `ArrayQueue` for `no_std`.
//!
//! Replaces `crossbeam-queue` (and its transitive deps: crossbeam-utils,
//! cfg-if, maybe-uninit, autocfg) with a single-file SPSC queue.
//!
//! Optimized for single-producer single-consumer (keyboard ISR → async task)
//! using `head`/`tail` atomic indices over a heap-allocated ring buffer.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct ArrayQueue<T> {
    head: AtomicUsize,
    tail: AtomicUsize,
    buffer: Box<[Slot<T>]>,
}

struct Slot<T> {
    value: UnsafeCell<MaybeUninit<T>>,
}

impl<T> ArrayQueue<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "ArrayQueue capacity must be > 0");
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(Slot {
                value: UnsafeCell::new(MaybeUninit::uninit()),
            });
        }
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            buffer: slots.into_boxed_slice(),
        }
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = tail.wrapping_add(1);

        if next_tail.wrapping_sub(self.head.load(Ordering::Acquire)) > self.buffer.len() {
            return Err(value);
        }

        let idx = tail % self.buffer.len();
        unsafe {
            (*self.buffer[idx].value.get()).write(value);
        }
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Result<T, PopError> {
        let head = self.head.load(Ordering::Relaxed);

        if head == self.tail.load(Ordering::Acquire) {
            return Err(PopError);
        }

        let idx = head % self.buffer.len();
        let value = unsafe { (*self.buffer[idx].value.get()).assume_init_read() };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(value)
    }

    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

unsafe impl<T: Send> Send for ArrayQueue<T> {}
unsafe impl<T: Send> Sync for ArrayQueue<T> {}

#[derive(Debug)]
pub struct PopError;
