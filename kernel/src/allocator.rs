pub mod bump;
pub mod fixed_size_block;
pub mod linked_list;
pub mod tlsf;

use crate::monitor;
use crate::trace::TraceEventId;
use alloc::alloc::{GlobalAlloc, Layout};
use core::{convert::TryFrom, ptr::null_mut};
use tlsf::TlsfAllocator;

pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 1_048_576; // 1 MB

#[global_allocator]
static ALLOCATOR: Locked<TlsfAllocator> = Locked::new(TlsfAllocator::new());

pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<'_, A> {
        self.inner.lock()
    }
}

unsafe impl GlobalAlloc for Locked<TlsfAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut tlsf = self.lock();
        let ptr = tlsf.malloc(layout.size(), layout.align());
        if !ptr.is_null() {
            monitor::inc_alloc(layout.size());
            crate::trace_event!(TraceEventId::Alloc, layout.size(), ptr as u64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.lock().free(ptr);
        monitor::inc_free(layout.size());
        crate::trace_event!(TraceEventId::Free, layout.size(), ptr as u64);
    }
}

pub struct Dummy;

unsafe impl GlobalAlloc for Dummy {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        panic!("dealloc should never be called")
    }
}

use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        // Include one extra page for the TLSF sentinel block at heap_end
        let heap_end = heap_start + u64::try_from(HEAP_SIZE).unwrap();
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    unsafe {
        ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}

/// Align the given address `addr` upwards to alignment `align`.
///
/// Requires that `align` is a power of two.
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
