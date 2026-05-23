use alloc::boxed::Box;
use alloc::vec;
use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
use core::ops::{Deref, DerefMut};
use x86_64::structures::paging::FrameAllocator;
use yonti_os::memory::buddy::BuddyAllocator;

struct MockAllocator {
    allocator: BuddyAllocator,
    buffer: *mut [u8],
    regions: *mut [MemoryRegion],
}

impl Deref for MockAllocator {
    type Target = BuddyAllocator;
    fn deref(&self) -> &Self::Target {
        &self.allocator
    }
}

impl DerefMut for MockAllocator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.allocator
    }
}

impl Drop for MockAllocator {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.buffer);
            let _ = Box::from_raw(self.regions);
        }
    }
}

fn create_mock_allocator(pages: usize) -> MockAllocator {
    // Allocate a page-aligned buffer of heap memory, with extra space for 64 KiB alignment
    let mut buffer = vec![0u8; (pages + 16) * 4096];
    let raw_addr = buffer.as_mut_ptr() as u64;
    let start_addr = (raw_addr + 65535) & !65535;

    let buffer_slice = buffer.into_boxed_slice();
    let buffer_ptr = Box::into_raw(buffer_slice);

    let mock_regions = vec![MemoryRegion {
        start: start_addr,
        end: start_addr + (pages as u64 * 4096),
        kind: MemoryRegionKind::Usable,
    }];
    let regions_slice = mock_regions.into_boxed_slice();
    let regions_ptr = Box::into_raw(regions_slice);

    let regions_ref: &'static mut [MemoryRegion] = unsafe { &mut *regions_ptr };
    let memory_regions = MemoryRegions::from(regions_ref);

    // Set physical_memory_offset = 0 so physical addresses map directly to virtual addresses of our buffer
    let allocator = BuddyAllocator::new(&memory_regions, 0);

    MockAllocator {
        allocator,
        buffer: buffer_ptr,
        regions: regions_ptr,
    }
}

#[test_case]
fn test_buddy_init() {
    let allocator = create_mock_allocator(64);
    assert_eq!(allocator.total_frames(), 64);
    assert_eq!(allocator.free_frames(), 64);
}

#[test_case]
fn test_buddy_alloc_dealloc_order_0() {
    let mut allocator = create_mock_allocator(64);
    let frame1 = allocator
        .allocate_frame()
        .expect("failed to allocate frame 1");
    let frame2 = allocator
        .allocate_frame()
        .expect("failed to allocate frame 2");

    assert_ne!(frame1.start_address(), frame2.start_address());
    assert_eq!(allocator.free_frames(), 62);

    allocator.deallocate_frame(frame1);
    allocator.deallocate_frame(frame2);
    assert_eq!(allocator.free_frames(), 64);
}

#[test_case]
fn test_buddy_alloc_high_order() {
    let mut allocator = create_mock_allocator(64);
    // Order 3 = 8 pages = 32 KiB
    let frame = allocator
        .allocate_frame_order(3)
        .expect("failed to allocate order 3 block");

    // Address must be aligned to 8 pages (32 KiB)
    let addr = frame.start_address().as_u64();
    assert_eq!(addr % (8 * 4096), 0);
    assert_eq!(allocator.free_frames(), 56);

    allocator.deallocate_frame_order(frame, 3);
    assert_eq!(allocator.free_frames(), 64);
}

#[test_case]
fn test_buddy_coalescing() {
    let mut allocator = create_mock_allocator(16);

    // Allocate all order 0 frames to exhaust the allocator
    let mut frames = vec![];
    for _ in 0..16 {
        frames.push(allocator.allocate_frame().unwrap());
    }
    assert_eq!(allocator.free_frames(), 0);

    // Deallocate them all
    for frame in frames {
        allocator.deallocate_frame(frame);
    }
    assert_eq!(allocator.free_frames(), 16);

    // If coalescing worked, we should be able to allocate an order 4 block (16 pages)
    let frame = allocator
        .allocate_frame_order(4)
        .expect("coalescing failed; cannot allocate order 4 block");
    allocator.deallocate_frame_order(frame, 4);
    assert_eq!(allocator.free_frames(), 16);
}

#[test_case]
fn test_buddy_exhaustion() {
    let mut allocator = create_mock_allocator(4);
    let f1 = allocator.allocate_frame().unwrap();
    let f2 = allocator.allocate_frame().unwrap();
    let f3 = allocator.allocate_frame().unwrap();
    let f4 = allocator.allocate_frame().unwrap();
    assert_eq!(allocator.free_frames(), 0);

    let f5 = allocator.allocate_frame();
    assert!(f5.is_none(), "allocator should be exhausted");

    allocator.deallocate_frame(f1);
    allocator.deallocate_frame(f2);
    allocator.deallocate_frame(f3);
    allocator.deallocate_frame(f4);
}
