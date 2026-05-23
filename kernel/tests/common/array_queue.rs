use yonti_os::array_queue::ArrayQueue;

#[test_case]
fn test_queue_simple() {
    let queue = ArrayQueue::new(10);
    assert!(queue.is_empty());

    queue.push(1).unwrap();
    queue.push(2).unwrap();
    queue.push(3).unwrap();
    assert!(!queue.is_empty());

    assert_eq!(queue.pop().unwrap(), 1);
    assert_eq!(queue.pop().unwrap(), 2);
    assert_eq!(queue.pop().unwrap(), 3);
    assert!(queue.is_empty());
}

#[test_case]
fn test_queue_limits() {
    let queue = ArrayQueue::new(3);

    // Fill queue
    queue.push(10).unwrap();
    queue.push(20).unwrap();
    queue.push(30).unwrap();

    // Try to push beyond capacity
    let res = queue.push(40);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), 40);

    // Pop everything
    assert_eq!(queue.pop().unwrap(), 10);
    assert_eq!(queue.pop().unwrap(), 20);
    assert_eq!(queue.pop().unwrap(), 30);

    // Pop empty queue
    let pop_res = queue.pop();
    assert!(pop_res.is_err());
}

#[test_case]
fn test_queue_wrap_around() {
    let queue = ArrayQueue::new(4);

    // Push/pop cycle many times
    for i in 0..1000 {
        queue.push(i).unwrap();
        assert_eq!(queue.pop().unwrap(), i);
    }
    assert!(queue.is_empty());
}
