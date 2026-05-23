//! TLSF (Two-Level Segregated Fit) heap allocator.
//!
//! O(1) worst-case allocation/deallocation. Two-level bitmaps for fast
//! free-block search, immediate boundary-tag coalescing on free.
//! MIN_BLOCK_SIZE=32, FL levels cover 32 B to 16 MiB.
//!
//! Block sizes are stored as `u32` — the maximum representable block size
//! is 4 GiB. The heap must not be enlarged beyond this limit without also
//! widening the size fields.

use crate::allocator::align_up;
use core::ptr::NonNull;

const HEADER_SIZE: usize = 8;
const MIN_BLOCK_SIZE: usize = 32;
const MIN_FL: usize = 5;
const FL_COUNT: usize = 19;
const SL_COUNT: usize = 32;
const FREE_FLAG: u32 = 1;
const PREV_FREE_FLAG: u32 = 2;

#[repr(C)]
struct BlockHeader {
    prev_phys_size: u32,
    size_and_flags: u32,
}

impl BlockHeader {
    fn size(&self) -> usize {
        (self.size_and_flags & !3) as usize
    }
    fn is_free(&self) -> bool {
        self.size_and_flags & FREE_FLAG != 0
    }
    fn prev_is_free(&self) -> bool {
        self.size_and_flags & PREV_FREE_FLAG != 0
    }
    fn set(&mut self, size: usize, free: bool, prev_free: bool) {
        debug_assert!(size <= u32::MAX as usize, "block size exceeds u32::MAX");
        self.size_and_flags = size as u32
            | if free { FREE_FLAG } else { 0 }
            | if prev_free { PREV_FREE_FLAG } else { 0 };
    }
    fn mark_free(&mut self, free: bool) {
        if free {
            self.size_and_flags |= FREE_FLAG;
        } else {
            self.size_and_flags &= !FREE_FLAG;
        }
    }
    fn mark_prev_free(&mut self, prev_free: bool) {
        if prev_free {
            self.size_and_flags |= PREV_FREE_FLAG;
        } else {
            self.size_and_flags &= !PREV_FREE_FLAG;
        }
    }
}

#[repr(C)]
struct FreeNode {
    prev: Option<NonNull<BlockHeader>>,
    next: Option<NonNull<BlockHeader>>,
}

#[derive(Debug)]
pub struct TlsfAllocator {
    heap_start: usize,
    heap_end: usize,
    fl_bitmap: u32,
    sl_bitmaps: [u32; FL_COUNT],
    free_lists: [[Option<NonNull<BlockHeader>>; SL_COUNT]; FL_COUNT],
}

// SAFETY: access is guarded by a spin::Mutex via the Locked wrapper.
unsafe impl Send for TlsfAllocator {}
unsafe impl Sync for TlsfAllocator {}

impl TlsfAllocator {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            heap_start: 0,
            heap_end: 0,
            fl_bitmap: 0,
            sl_bitmaps: [0; FL_COUNT],
            free_lists: [[None; SL_COUNT]; FL_COUNT],
        }
    }

    /// # Safety
    /// Must be called exactly once with a valid, unused memory region.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        unsafe {
            self.heap_start = heap_start;
            self.heap_end = heap_start + heap_size;

            let block = heap_start as *mut BlockHeader;
            (*block).prev_phys_size = 0;
            (*block).set(heap_size, true, false);

            let sentinel = (heap_start + heap_size) as *mut BlockHeader;
            (*sentinel).prev_phys_size = heap_size as u32;
            (*sentinel).set(0, false, true);

            let node = NonNull::new_unchecked(block);
            self.insert(node, heap_size);
        }
    }

    /// Allocate a block of at least `size` bytes with the given alignment.
    /// `align` must be a power of two.
    pub fn malloc(&mut self, size: usize, align: usize) -> *mut u8 {
        let align = align.max(HEADER_SIZE);

        // The header is 8 bytes. After alignment padding, the user pointer
        // may be shifted forward. The worst case is we waste `align - 1` bytes
        // before the user data. We need a block large enough for:
        //   header + max_alignment_pad + size
        let worst_padding = align.saturating_sub(HEADER_SIZE);
        let needed = align_up(HEADER_SIZE + worst_padding + size, 8).max(MIN_BLOCK_SIZE);

        let (fli, sl) = Self::mapping(needed);
        let fli = fli.min(FL_COUNT - 1);

        let block = match self.find_block(fli, sl, needed) {
            Some(b) => b,
            None => return core::ptr::null_mut(),
        };
        self.alloc_from(block, size, align)
    }

    /// # Safety
    ///
    /// `ptr` must have been returned by a prior `malloc()` call and not
    /// already freed. Passing a null pointer is safe (no-op).
    pub unsafe fn free(&mut self, ptr: *mut u8) {
        unsafe {
            if ptr.is_null() {
                return;
            }
            let mut header = (ptr as usize - HEADER_SIZE) as *mut BlockHeader;
            let mut size = (*header).size();
            let mut addr = header as usize;

            (*header).mark_free(true);

            // forward coalesce
            let next_ptr = (addr + size) as *mut BlockHeader;
            if (next_ptr as usize) < self.heap_end && (*next_ptr).is_free() {
                self.remove(NonNull::new_unchecked(next_ptr));
                size += (*next_ptr).size();
                (*header).set(size, true, (*header).prev_is_free());
            }

            // backward coalesce
            if (*header).prev_is_free() {
                let prev_size = (*header).prev_phys_size as usize;
                let prev = (addr - prev_size) as *mut BlockHeader;
                self.remove(NonNull::new_unchecked(prev));
                size += prev_size;
                header = prev;
                addr = prev as usize;
                (*prev).set(size, true, (*prev).prev_is_free());
            }

            // update next block
            let next = (addr + size) as *mut BlockHeader;
            (*next).prev_phys_size = size as u32;
            (*next).mark_prev_free(true);

            self.insert(NonNull::new_unchecked(header), size);
        }
    }

    // ── internal helpers ────────────────────────────────────────

    fn mapping(size: usize) -> (usize, usize) {
        let fl = (usize::BITS - size.leading_zeros() - 1) as usize;
        let sl = ((size ^ (1 << fl)) * SL_COUNT) >> fl;
        (
            fl.saturating_sub(MIN_FL).min(FL_COUNT - 1),
            sl.min(SL_COUNT - 1),
        )
    }

    /// Find a free block that satisfies the given size class, iterating
    /// through coarser buckets if the exact bucket has an undersized block.
    fn find_block(&mut self, fli: usize, sl: usize, needed: usize) -> Option<NonNull<BlockHeader>> {
        // Try all SL buckets from `sl` upward in this FL
        let sl_map = self.sl_bitmaps[fli] & (!0u32 << sl);
        if sl_map != 0 {
            let start = sl_map.trailing_zeros() as usize;
            for sl2 in start..SL_COUNT {
                if self.sl_bitmaps[fli] & (1u32 << sl2) != 0 {
                    // Peek at the head block — verify it's large enough
                    if let Some(b) = self.free_lists[fli][sl2] {
                        let size = unsafe { (*b.as_ptr()).size() };
                        if size >= needed {
                            return Some(self.pop(fli, sl2));
                        }
                    }
                }
            }
        }

        // Search higher FLs
        let fl_map = self.fl_bitmap & (!0u32 << (fli + 1));
        if fl_map != 0 {
            let fl2 = fl_map.trailing_zeros() as usize;
            let sl2 = self.sl_bitmaps[fl2].trailing_zeros() as usize;
            return Some(self.pop(fl2, sl2));
        }
        None
    }

    fn alloc_from(
        &mut self,
        block: NonNull<BlockHeader>,
        user_size: usize,
        align: usize,
    ) -> *mut u8 {
        let header = block.as_ptr();
        let block_size = unsafe { (*header).size() };
        let block_addr = header as usize;

        // Compute the aligned user-data pointer within this block.
        // User data starts at `header + HEADER_SIZE`, but may need
        // front-padding to satisfy the alignment requirement.
        let user_start = block_addr + HEADER_SIZE;
        let aligned_start = align_up(user_start, align);
        let front_padding = aligned_start - user_start;

        // Total block size needed: header + front_padding + user_size
        // Ensure total_consumed is aligned to 8 bytes and at least MIN_BLOCK_SIZE to keep blocks aligned.
        let total_consumed =
            align_up(HEADER_SIZE + front_padding + user_size, 8).max(MIN_BLOCK_SIZE);

        // Verify the block is large enough (defense against TLSF bucket imprecision)
        if block_size < total_consumed {
            // Block too small — insert it back and return null.
            // This should not happen because find_block verified size >= needed,
            // but `needed` includes worst-case padding. If the actual alignment
            // padding makes the block too small, we can't satisfy this request.
            unsafe {
                (*header).set(block_size, true, (*header).prev_is_free());
            }
            self.insert(block, block_size);
            return core::ptr::null_mut();
        }

        let effective_total = total_consumed;

        if block_size >= effective_total + MIN_BLOCK_SIZE {
            // Split remainder after the allocation
            let rem_addr = block_addr + effective_total;
            let rem_size = block_size - effective_total;
            unsafe {
                (*header).set(effective_total, false, (*header).prev_is_free());

                let rem = rem_addr as *mut BlockHeader;
                (*rem).prev_phys_size = effective_total as u32;
                (*rem).set(rem_size, true, false);

                let next = (rem_addr + rem_size) as *mut BlockHeader;
                (*next).prev_phys_size = rem_size as u32;
                (*next).mark_prev_free(true);

                self.insert(NonNull::new_unchecked(rem), rem_size);
            }
        } else {
            unsafe {
                (*header).mark_free(false);
                let next = (block_addr + block_size) as *mut BlockHeader;
                (*next).mark_prev_free(false);
            }
        }

        // If we needed front-padding, split it off as a free block
        if front_padding > 0 {
            // The front_padding region (after header, before aligned_start) is wasted.
            // We already accounted for it in `effective_total`, so the allocation
            // starts at block_addr and includes the padding. The user pointer is
            // aligned_start. The padding is not separately freed — it's part of
            // this allocation's overhead.
        }

        aligned_start as *mut u8
    }

    fn insert(&mut self, block: NonNull<BlockHeader>, size: usize) {
        let (fli, sl) = Self::mapping(size);
        unsafe {
            let node = (block.as_ptr().add(1)) as *mut FreeNode;
            if let Some(head) = self.free_lists[fli][sl] {
                (*node).prev = None;
                (*node).next = Some(head);
                let hn = (head.as_ptr().add(1)) as *mut FreeNode;
                (*hn).prev = Some(block);
            } else {
                (*node).prev = None;
                (*node).next = None;
            }
            self.free_lists[fli][sl] = Some(block);
        }
        self.sl_bitmaps[fli] |= 1u32 << sl;
        self.fl_bitmap |= 1u32 << fli;
    }

    fn pop(&mut self, fli: usize, sl: usize) -> NonNull<BlockHeader> {
        let block = self.free_lists[fli][sl].expect("pop from empty free list");
        self.remove(block);
        block
    }

    fn remove(&mut self, block: NonNull<BlockHeader>) {
        let size = unsafe { (*block.as_ptr()).size() };
        let (fli, sl) = Self::mapping(size);
        unsafe {
            let node = (block.as_ptr().add(1)) as *mut FreeNode;
            match ((*node).prev, (*node).next) {
                (None, None) => {
                    self.free_lists[fli][sl] = None;
                    self.sl_bitmaps[fli] &= !(1u32 << sl);
                    if self.sl_bitmaps[fli] == 0 {
                        self.fl_bitmap &= !(1u32 << fli);
                    }
                }
                (None, Some(next)) => {
                    self.free_lists[fli][sl] = Some(next);
                    let nn = (next.as_ptr().add(1)) as *mut FreeNode;
                    (*nn).prev = None;
                }
                (Some(prev), None) => {
                    let pn = (prev.as_ptr().add(1)) as *mut FreeNode;
                    (*pn).next = None;
                }
                (Some(prev), Some(next)) => {
                    let pn = (prev.as_ptr().add(1)) as *mut FreeNode;
                    let nn = (next.as_ptr().add(1)) as *mut FreeNode;
                    (*pn).next = Some(next);
                    (*nn).prev = Some(prev);
                }
            }
        }
    }
}
