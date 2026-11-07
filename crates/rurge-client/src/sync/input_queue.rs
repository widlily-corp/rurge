use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    KeyDown(u32),
    KeyUp(u32),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Scroll(i32),
}

#[derive(Clone)]
pub struct InputQueue {
    inner: Arc<ArrayQueue<InputEvent>>,
}

impl InputQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    pub fn push(&self, event: InputEvent) -> bool {
        self.inner.push(event).is_ok()
    }

    pub fn pop(&self) -> Option<InputEvent> {
        self.inner.pop()
    }

    pub fn drain_into(&self, target: &mut Vec<InputEvent>) {
        while let Some(event) = self.inner.pop() {
            target.push(event);
        }
    }
}
