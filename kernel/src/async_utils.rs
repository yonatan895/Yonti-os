//! Minimal async utilities replacing futures-util.
//!
//! Replaces `futures-util`, `futures-core`, `futures-task`, `pin-project-lite`,
//! and `slab` with local implementations tailored to a single-threaded kernel.
//!
//! The `AtomicWaker` here is simplified compared to futures-core: since the
//! kernel runs on a single core with cooperative scheduling (interrupts run to
//! completion before returning), we don't need CAS-based lock-free concurrency
//! — `UnsafeCell` suffices.

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
        unsafe {
            *self.waker.get() = Some(waker.clone());
        }
    }

    pub fn wake(&self) {
        if let Some(waker) = self.take() {
            waker.wake();
        }
    }

    pub fn take(&self) -> Option<Waker> {
        unsafe { (*self.waker.get()).take() }
    }
}

unsafe impl Send for AtomicWaker {}
unsafe impl Sync for AtomicWaker {}
