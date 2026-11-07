use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

const DIRTY_BIT: u8 = 0b0000_0100;
const INDEX_MASK: u8 = 0b0000_0011;

struct Shared<T> {
    slots: [UnsafeCell<T>; 3],
    state: AtomicU8,
}

unsafe impl<T: Send> Send for Shared<T> {}
unsafe impl<T: Send> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
    back: usize,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
    front: usize,
}

pub fn triple_buffer<T>(factory: impl Fn() -> T) -> (Producer<T>, Consumer<T>) {
    let shared = Arc::new(Shared {
        slots: [
            UnsafeCell::new(factory()),
            UnsafeCell::new(factory()),
            UnsafeCell::new(factory()),
        ],
        // Initial state: buffer 1 is shared and clean (not dirty)
        state: AtomicU8::new(1),
    });

    (
        Producer {
            shared: Arc::clone(&shared),
            back: 0,
        },
        Consumer {
            shared,
            front: 2,
        },
    )
}

impl<T> Producer<T> {
    pub fn write(&mut self, update: impl FnOnce(&mut T)) {
        let slot = unsafe { &mut *self.shared.slots[self.back].get() };
        update(slot);
        let new_state = (self.back as u8) | DIRTY_BIT;
        let old_state = self.shared.state.swap(new_state, Ordering::AcqRel);
        self.back = (old_state & INDEX_MASK) as usize;
    }
}

impl<T> Consumer<T> {
    pub fn read(&mut self) -> Option<&T> {
        let mut current = self.shared.state.load(Ordering::Acquire);
        loop {
            if current & DIRTY_BIT == 0 {
                return None;
            }
            let target_idx = (current & INDEX_MASK) as usize;
            let new_state = self.front as u8; // clean (DIRTY_BIT = 0)
            match self.shared.state.compare_exchange_weak(
                current,
                new_state,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.front = target_idx;
                    return Some(unsafe { &*self.shared.slots[self.front].get() });
                }
                Err(actual) => {
                    current = actual;
                }
            }
        }
    }
}
