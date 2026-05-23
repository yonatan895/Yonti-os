//! Buddy physical frame allocator.
//!
//! Manages physical pages in power-of-two blocks (order 0 = 4 KiB up to
//! order 10 = 4 MiB). Free blocks are tracked via a singly-linked list
//! threaded through the free pages themselves (zero overhead when allocated).
//! A dense bitmap tracks which frames are free vs. allocated.

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};
use x86_64::PhysAddr;

use crate::monitor;

const MAX_ORDER: usize = 10;
const MAX_TRACKED_FRAMES: usize = 131_072; // up to 512 MiB
const BITMAP_U64_LEN: usize = MAX_TRACKED_FRAMES / 64;
/// Sentinel value for free-list links. Must never be a valid frame index.
const NULL_LINK: usize = usize::MAX;

pub struct BuddyAllocator {
    free_lists: [Option<usize>; MAX_ORDER + 1],
    bitmap: [u64; BITMAP_U64_LEN],
    total_frames: usize,
    /// Physical address of frame index 0.
    base_phys: u64,
    /// Offset at which physical memory is identity-mapped.
    phys_mem_offset: u64,
    allocated_count: usize,
}

impl BuddyAllocator {
    pub fn new(memory_regions: &MemoryRegions, physical_memory_offset: u64) -> Self {
        let mut allocator = Self {
            free_lists: [None; MAX_ORDER + 1],
            bitmap: [0; BITMAP_U64_LEN],
            total_frames: 0,
            base_phys: u64::MAX,
            phys_mem_offset: physical_memory_offset,
            allocated_count: 0,
        };

        // Find usable ranges and compute total frames + base address
        let mut base = u64::MAX;
        let mut last_end = 0u64;
        let mut found_usable = false;

        for r in memory_regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
        {
            found_usable = true;
            base = base.min(r.start);
            last_end = last_end.max(r.end);
        }
        assert!(found_usable, "no usable memory regions");

        allocator.base_phys = base;
        let total_frames = (last_end - base) as usize / 4096;
        assert!(
            total_frames <= MAX_TRACKED_FRAMES,
            "too many physical frames to track"
        );
        allocator.total_frames = total_frames;
        monitor::set_frame_metrics(total_frames);

        // Mark all usable frames as free
        for r in memory_regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
        {
            let start_idx = ((r.start - allocator.base_phys) / 4096) as usize;
            let end_idx = ((r.end - allocator.base_phys) / 4096) as usize;
            for idx in start_idx..end_idx {
                allocator.mark_free(idx);
            }
        }

        // Build free lists top-down: carve each usable range into max-order blocks
        for r in memory_regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
        {
            let mut pos = ((r.start - allocator.base_phys) / 4096) as usize;
            let end = ((r.end - allocator.base_phys) / 4096) as usize;

            while pos < end {
                let remaining = end - pos;
                let mut order = MAX_ORDER.min(floor_log2(remaining));

                // Find the largest aligned order that fits
                while order > 0 && !allocator.is_aligned_for_order(pos, order) {
                    order -= 1;
                }
                // Skip over non-free frames (e.g., inside a non-usable gap)
                if allocator.is_free(pos) {
                    allocator.insert_free(pos, order);
                }
                pos += 1 << order;
            }
        }

        allocator
    }

    // ── Bitmap helpers ────────────────────────────────────────────

    fn bitmap_index(idx: usize) -> (usize, u64) {
        (idx / 64, 1u64 << (idx % 64))
    }

    fn is_free(&self, idx: usize) -> bool {
        let (word, bit) = Self::bitmap_index(idx);
        self.bitmap[word] & bit != 0
    }

    fn is_range_free(&self, start: usize, count: usize) -> bool {
        for i in 0..count {
            if !self.is_free(start + i) {
                return false;
            }
        }
        true
    }

    fn mark_free(&mut self, idx: usize) {
        let (word, bit) = Self::bitmap_index(idx);
        self.bitmap[word] |= bit;
    }

    fn mark_allocated(&mut self, idx: usize) {
        let (word, bit) = Self::bitmap_index(idx);
        self.bitmap[word] &= !bit;
    }

    fn mark_range_allocated(&mut self, start: usize, count: usize) {
        for i in 0..count {
            self.mark_allocated(start + i);
        }
    }

    fn mark_range_free(&mut self, start: usize, count: usize) {
        for i in 0..count {
            self.mark_free(start + i);
        }
    }

    // ── Free list helpers ─────────────────────────────────────────

    fn is_aligned_for_order(&self, idx: usize, order: usize) -> bool {
        (idx & ((1 << order) - 1)) == 0
    }

    fn insert_free(&mut self, idx: usize, order: usize) {
        debug_assert!(self.is_aligned_for_order(idx, order));
        self.mark_range_free(idx, 1 << order);

        let header = self.frame_idx_to_ptr(idx);
        // Thread the free list through the first page of the block
        unsafe {
            *(header as *mut usize) = self.free_lists[order].unwrap_or(NULL_LINK);
        }
        self.free_lists[order] = Some(idx);
    }

    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let idx = self.free_lists[order]?;
        let next = unsafe { *(self.frame_idx_to_ptr(idx) as *const usize) };
        self.free_lists[order] = if next == NULL_LINK { None } else { Some(next) };
        self.mark_range_allocated(idx, 1 << order);
        Some(idx)
    }

    fn remove_free(&mut self, idx: usize, order: usize) {
        // Walk free list for this order, remove matching entry
        let mut prev: Option<usize> = None;
        let mut current = self.free_lists[order];

        while let Some(cur_idx) = current {
            if cur_idx == idx {
                let next = unsafe { *(self.frame_idx_to_ptr(cur_idx) as *const usize) };
                let next = if next == NULL_LINK { None } else { Some(next) };
                match prev {
                    Some(p) => unsafe {
                        *(self.frame_idx_to_ptr(p) as *mut usize) = next.unwrap_or(NULL_LINK);
                    },
                    None => {
                        self.free_lists[order] = next;
                    }
                }
                return;
            }
            prev = current;
            let raw_next = unsafe { *(self.frame_idx_to_ptr(cur_idx) as *const usize) };
            current = if raw_next == NULL_LINK {
                None
            } else {
                Some(raw_next)
            };
        }
    }

    // ── Address conversion ────────────────────────────────────────

    fn idx_to_phys(&self, idx: usize) -> u64 {
        self.base_phys + (idx as u64 * 4096)
    }

    fn phys_to_idx(&self, phys: u64) -> usize {
        ((phys - self.base_phys) / 4096) as usize
    }

    fn frame_idx_to_ptr(&self, idx: usize) -> *mut u8 {
        (self.idx_to_phys(idx) + self.phys_mem_offset) as *mut u8
    }

    // ── Allocation / Deallocation ─────────────────────────────────

    pub fn allocate_frame_order(&mut self, order: usize) -> Option<PhysFrame<Size4KiB>> {
        if order > MAX_ORDER {
            return None;
        }

        // Find smallest available order >= requested
        let mut alloc_order = order;
        while alloc_order <= MAX_ORDER && self.free_lists[alloc_order].is_none() {
            alloc_order += 1;
        }
        if alloc_order > MAX_ORDER {
            return None;
        }

        let idx = self.pop_free(alloc_order)?;

        // Split down to requested order, returning buddies as free
        while alloc_order > order {
            alloc_order -= 1;
            let buddy = idx + (1 << alloc_order);
            self.insert_free(buddy, alloc_order);
        }

        self.allocated_count += 1 << order;
        monitor::inc_allocated_frames(1 << order);
        Some(PhysFrame::containing_address(PhysAddr::new(
            self.idx_to_phys(idx),
        )))
    }

    pub fn deallocate_frame_order(&mut self, frame: PhysFrame<Size4KiB>, mut order: usize) {
        if order > MAX_ORDER {
            return;
        }

        let mut idx = self.phys_to_idx(frame.start_address().as_u64());

        // Coalesce upward while buddy is free
        while order < MAX_ORDER {
            let buddy_idx = idx ^ (1 << order);
            if buddy_idx >= self.total_frames {
                break;
            }
            if !self.is_range_free(buddy_idx, 1 << order) {
                break;
            }
            self.remove_free(buddy_idx, order);
            self.mark_range_allocated(buddy_idx, 1 << order);
            idx = idx.min(buddy_idx);
            order += 1;
        }

        self.insert_free(idx, order);
        self.allocated_count -= 1 << order;
        monitor::dec_allocated_frames(1 << order);
    }

    pub fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.deallocate_frame_order(frame, 0);
    }

    pub fn free_frames(&self) -> usize {
        self.total_frames - self.allocated_count
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }
}

unsafe impl FrameAllocator<Size4KiB> for BuddyAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate_frame_order(0)
    }
}

fn floor_log2(x: usize) -> usize {
    if x == 0 {
        return 0;
    }
    (usize::BITS - x.leading_zeros() - 1) as usize
}
