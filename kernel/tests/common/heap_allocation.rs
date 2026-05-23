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
