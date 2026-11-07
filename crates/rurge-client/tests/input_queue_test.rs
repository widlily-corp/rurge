use rurge_client::sync::input_queue::{InputEvent, InputQueue, MouseButton};

#[test]
fn test_input_queue_fifo_order() {
    let queue = InputQueue::new(64);
    assert!(queue.push(InputEvent::KeyDown(32))); // Space
    assert!(queue.push(InputEvent::MouseDown(MouseButton::Left)));

    assert_eq!(queue.pop(), Some(InputEvent::KeyDown(32)));
    assert_eq!(queue.pop(), Some(InputEvent::MouseDown(MouseButton::Left)));
    assert_eq!(queue.pop(), None);
}

#[test]
fn test_input_queue_drain_batch() {
    let queue = InputQueue::new(64);
    for i in 0..10 {
        queue.push(InputEvent::KeyDown(i));
    }
    let mut batch = Vec::new();
    queue.drain_into(&mut batch);
    assert_eq!(batch.len(), 10);
    assert_eq!(batch[0], InputEvent::KeyDown(0));
    assert_eq!(batch[9], InputEvent::KeyDown(9));
}
