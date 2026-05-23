//! Minimal async utilities replacing futures-util.
//!
//! Replaces `futures-util`, `futures-core`, `futures-task`, `pin-project-lite`,
//! and `slab` with local implementations tailored to a single-threaded kernel.
//!
//! The `AtomicWaker` uses `UnsafeCell` but disables interrupts during
//! register/take to prevent data races with the keyboard ISR, which calls
//! `wake()` from interrupt context.

use core::cell::UnsafeCell;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

// ---------------------------------------------------------------------------
// Stream trait
// ---------------------------------------------------------------------------

pub trait Stream {
    type Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>>;
}

// ---------------------------------------------------------------------------
// StreamExt — extension trait with .next()
// ---------------------------------------------------------------------------

pub trait StreamExt: Stream {
    fn next(&mut self) -> Next<'_, Self>
    where
        Self: Unpin,
    {
        Next { stream: self }
    }
}

impl<T: ?Sized + Stream> StreamExt for T {}

pub struct Next<'a, St: ?Sized> {
    stream: &'a mut St,
}

impl<St: ?Sized + Stream + Unpin> core::future::Future for Next<'_, St> {
    type Output = Option<St::Item>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.stream).poll_next(cx)
    }
}

impl<St: ?Sized + Stream + Unpin> Unpin for Next<'_, St> {}

// ---------------------------------------------------------------------------
// AtomicWaker — simplified for single-core cooperative kernel
// ---------------------------------------------------------------------------

pub struct AtomicWaker {
    waker: UnsafeCell<Option<Waker>>,
}

impl AtomicWaker {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            waker: UnsafeCell::new(None),
        }
    }

    pub fn register(&self, waker: &Waker) {
        // Disable interrupts to prevent the keyboard ISR from calling
        // wake()/take() while we're modifying the waker cell.
        x86_64::instructions::interrupts::without_interrupts(|| unsafe {
            *self.waker.get() = Some(waker.clone());
        });
    }

    pub fn wake(&self) {
        if let Some(waker) = self.take() {
            waker.wake();
        }
    }

    pub fn take(&self) -> Option<Waker> {
        x86_64::instructions::interrupts::without_interrupts(|| unsafe {
            (*self.waker.get()).take()
        })
    }
}

unsafe impl Send for AtomicWaker {}
unsafe impl Sync for AtomicWaker {}
