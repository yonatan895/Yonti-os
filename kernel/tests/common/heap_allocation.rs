use alloc::alloc::{Layout, alloc, dealloc};
use alloc::boxed::Box;
use alloc::vec::Vec;
use yonti_os::allocator::HEAP_SIZE;

#[test_case]
fn simple_allocation() {
    let heap_value_1 = Box::new(50);
    assert_eq!(*heap_value_1, 50);
}

#[test_case]
fn large_vec() {
    let n = 10000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2);
}

#[test_case]
fn many_boxes() {
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

#[test_case]
fn many_boxes_long_lived() {
    let long_lived = Box::new(1);
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    assert_eq!(*long_lived, 1);
}

#[test_case]
fn test_alloc_alignments() {
    let alignments = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];
    for &align in &alignments {
        let layout = Layout::from_size_align(128, align).unwrap();
        let ptr = unsafe { alloc(layout) };
        assert!(
            !ptr.is_null(),
            "failed to allocate with alignment {}",
            align
        );
        assert_eq!(
            ptr as usize % align,
            0,
            "pointer {:?} not aligned to {}",
            ptr,
            align
        );
        unsafe { dealloc(ptr, layout) };
    }
}

#[test_case]
fn test_tlsf_coalescing() {
    let layout = Layout::from_size_align(1024, 8).unwrap();
    // Allocate 3 blocks
    let ptr1 = unsafe { alloc(layout) };
    let ptr2 = unsafe { alloc(layout) };
    let ptr3 = unsafe { alloc(layout) };

    assert!(!ptr1.is_null());
    assert!(!ptr2.is_null());
    assert!(!ptr3.is_null());

    // Free all of them to trigger coalescing
    unsafe {
        dealloc(ptr1, layout);
        dealloc(ptr2, layout);
        dealloc(ptr3, layout);
    }

    // Now we should be able to allocate a single block of size 3072 bytes
    let large_layout = Layout::from_size_align(3072, 8).unwrap();
    let large_ptr = unsafe { alloc(large_layout) };
    assert!(
        !large_ptr.is_null(),
        "Coalescing failed to merge freed blocks into a single large block"
    );
    unsafe { dealloc(large_ptr, large_layout) };
}

#[test_case]
fn test_massive_allocation_failure() {
    // Attempt to allocate a block larger than the heap size
    let layout = Layout::from_size_align(HEAP_SIZE * 2, 8).unwrap();
    let ptr = unsafe { alloc(layout) };
    assert!(
        ptr.is_null(),
        "allocating double HEAP_SIZE should return a null pointer"
    );
}
