//! Type-safe, lock-free communication channels
//!
//! This module provides `Channel<T>`, a bidirectional communication primitive
//! built on top of lock-free ring buffers.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Channel error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelError {
    /// Channel is full (send would block)
    Full,
    /// Channel is empty (receive would block)
    Empty,
    /// Channel is closed
    Closed,
}

/// Lock-free ring buffer for SPSC/MPSC communication
struct RingBuffer<T> {
    /// Buffer storage (uninitialized until written)
    /// UnsafeCell allows interior mutability for lock-free operations
    buffer: Vec<UnsafeCell<MaybeUninit<T>>>,
    /// Head index (consumer reads here)
    head: AtomicU32,
    /// Tail index (producer writes here)
    tail: AtomicU32,
    /// Capacity mask (size - 1, for wrapping)
    mask: u32,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with given capacity (must be power of 2)
    fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be power of 2");
        assert!(capacity <= (u32::MAX as usize), "Capacity too large");

        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(UnsafeCell::new(MaybeUninit::uninit()));
        }

        Self {
            buffer,
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            mask: (capacity - 1) as u32,
        }
    }

    /// Try to push a value (non-blocking)
    fn push(&self, value: T) -> Result<(), ChannelError> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        // Check if full (tail + 1 == head)
        let next_tail = (tail + 1) & self.mask;
        if next_tail == head {
            return Err(ChannelError::Full);
        }

        // Write value
        let index = tail as usize;
        unsafe {
            // Get mutable pointer through UnsafeCell
            let cell_ptr = self.buffer[index].get();
            (*cell_ptr).as_mut_ptr().write(value);
        }

        // Commit write
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }

    /// Try to pop a value (non-blocking)
    fn pop(&self) -> Result<T, ChannelError> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        // Check if empty
        if head == tail {
            return Err(ChannelError::Empty);
        }

        // Read value
        let index = head as usize;
        let value = unsafe {
            let cell_ptr = self.buffer[index].get();
            (*cell_ptr).as_ptr().read()
        };

        // Commit read
        let next_head = (head + 1) & self.mask;
        self.head.store(next_head, Ordering::Release);

        Ok(value)
    }

    /// Get the number of elements currently in the buffer
    fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if tail >= head {
            (tail - head) as usize
        } else {
            ((self.mask + 1) - head + tail) as usize
        }
    }

    /// Check if the buffer is empty
    fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    /// Check if the buffer is full
    fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        let next_tail = (tail + 1) & self.mask;
        next_tail == head
    }
}

impl<T> Drop for RingBuffer<T> {
    fn drop(&mut self) {
        // Properly drop all initialized values
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Relaxed);

        let mut current = head;
        while current != tail {
            let index = current as usize;
            unsafe {
                let cell_ptr = self.buffer[index].get();
                (*cell_ptr).as_mut_ptr().drop_in_place();
            }
            current = (current + 1) & self.mask;
        }
    }
}

/// Channel state
struct ChannelState {
    /// Closed flag
    closed: AtomicBool,
    /// Capacity
    capacity: usize,
}

/// Type-safe bidirectional communication channel
///
/// `Channel<T>` provides lock-free bidirectional communication
/// between two endpoints. Each endpoint can both send and receive.
pub struct Channel<T> {
    /// Ring buffer for data
    ring: Arc<RingBuffer<T>>,
    /// Channel state
    state: Arc<ChannelState>,
}

impl<T> Channel<T> {
    /// Create a new channel with given capacity
    ///
    /// # Panics
    ///
    /// Panics if capacity is not a power of 2.
    pub fn new(capacity: usize) -> Self {
        Self {
            ring: Arc::new(RingBuffer::new(capacity)),
            state: Arc::new(ChannelState {
                closed: AtomicBool::new(false),
               capacity,
            }),
        }
    }

    /// Send a value through the channel (non-blocking)
    ///
    /// # Errors
    ///
    /// Returns `ChannelError::Full` if the channel is full.
    /// Returns `ChannelError::Closed` if the channel is closed.
    pub fn send(&self, value: T) -> Result<(), ChannelError> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(ChannelError::Closed);
        }

        self.ring.push(value)
    }

    /// Receive a value from the channel (non-blocking)
    ///
    /// # Errors
    ///
    /// Returns `ChannelError::Empty` if the channel is empty.
    /// Returns `ChannelError::Closed` if the channel is closed and empty.
    pub fn recv(&self) -> Result<T, ChannelError> {
        match self.ring.pop() {
            Ok(value) => Ok(value),
            Err(ChannelError::Empty) => {
                if self.state.closed.load(Ordering::Acquire) {
                    Err(ChannelError::Closed)
                } else {
                    Err(ChannelError::Empty)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Close the channel
    ///
    /// After closing, no more sends are allowed, but remaining
    /// values can still be received.
    pub fn close(&self) {
        self.state.closed.store(true, Ordering::Release);
    }

    /// Check if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.state.closed.load(Ordering::Acquire)
    }

    /// Get the number of elements currently in the channel
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// Check if the channel is empty
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// Check if the channel is full
    pub fn is_full(&self) -> bool {
        self.ring.is_full()
    }

    /// Get the channel capacity
    pub fn capacity(&self) -> usize {
        self.state.capacity
    }
}

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Self {
            ring: Arc::clone(&self.ring),
            state: Arc::clone(&self.state),
        }
    }
}

// Safety: Channel<T> is Send + Sync if T is Send + Sync
unsafe impl<T: Send> Send for Channel<T> {}
unsafe impl<T: Send> Sync for Channel<T> {}
