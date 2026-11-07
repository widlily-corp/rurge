use bumpalo::Bump;

pub struct TickArena {
    inner: Bump,
}

impl TickArena {
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            inner: Bump::with_capacity(bytes),
        }
    }

    pub fn alloc_slice_copy<T: Copy>(&self, src: &[T]) -> &mut [T] {
        self.inner.alloc_slice_copy(src)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
