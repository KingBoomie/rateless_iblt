use crate::RibltError;

/// A fixed-capacity ring buffer that tracks unique indices.
/// Used internally by `decode_all` to track peelable blocks.
pub struct IndexQueue<const N: usize> {
    buffer: [usize; N],
    head: usize,
    tail: usize,
    len: usize,
    /// Bitmask equivalent to track if an item is already queued to prevent cycles/duplicates.
    in_queue: [bool; N],
}

impl<const N: usize> IndexQueue<N> {
    pub fn new() -> Self {
        Self {
            buffer: [0; N],
            head: 0,
            tail: 0,
            len: 0,
            in_queue: [false; N],
        }
    }

    /// Pushes an index if it is not already in the queue.
    pub fn push_unique(&mut self, idx: usize) -> Result<(), RibltError> {
        if idx >= N {
            // Should be unreachable if logic is correct, but safety first.
            return Err(RibltError::QueueOverflow);
        }
        if self.in_queue[idx] {
            return Ok(());
        }
        if self.len >= N {
            return Err(RibltError::QueueOverflow);
        }

        self.buffer[self.head] = idx;
        self.head = (self.head + 1) % N;
        self.len += 1;
        self.in_queue[idx] = true;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<usize> {
        if self.len == 0 {
            None
        } else {
            let idx = self.buffer[self.tail];
            self.tail = (self.tail + 1) % N;
            self.len -= 1;
            // Important: we remove the 'in_queue' marker so it can be re-added if necessary
            // (though in peeling, usually a block is processed once, but this preserves original logic).
            self.in_queue[idx] = false;
            Some(idx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_queue_basic_push_pop() {
        const CAP: usize = 5;
        let mut q = IndexQueue::<CAP>::new();

        assert!(q.push_unique(1).is_ok());
        assert!(q.push_unique(2).is_ok());
        assert!(q.push_unique(3).is_ok());

        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(3));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn test_index_queue_deduplication() {
        const CAP: usize = 5;
        let mut q = IndexQueue::<CAP>::new();

        assert!(q.push_unique(1).is_ok());
        // Should succeed but do nothing internal state-wise regarding count
        assert!(q.push_unique(1).is_ok());

        assert_eq!(q.len, 1);
        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn test_index_queue_wrapping() {
        const CAP: usize = 3;
        let mut q = IndexQueue::<CAP>::new();

        // Fill
        assert!(q.push_unique(0).is_ok());
        assert!(q.push_unique(1).is_ok());
        assert!(q.push_unique(2).is_ok());

        // Pop one to make space at head
        assert_eq!(q.pop(), Some(0)); // Tail moves

        // Push new (should wrap to index 0 in buffer)
        assert!(q.push_unique(0).is_ok());

        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
        assert_eq!(q.pop(), Some(0));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn test_index_queue_overflow() {
        const CAP: usize = 2;
        let mut q = IndexQueue::<CAP>::new();

        assert!(q.push_unique(0).is_ok());
        assert!(q.push_unique(1).is_ok());

        // Queue is full
        match q.push_unique(2) {
            Err(RibltError::QueueOverflow) => {}
            _ => panic!("Should have overflowed"),
        }
    }
}
