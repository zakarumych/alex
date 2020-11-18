use {
    crate::util::capacity_overflow,
    alloc::{
        alloc::{alloc, handle_alloc_error},
        vec::Vec,
    },
    core::{
        alloc::Layout,
        marker::PhantomData,
        mem::size_of,
        ptr::{
            copy_nonoverlapping, drop_in_place, null_mut, read, slice_from_raw_parts_mut, write,
        },
        sync::atomic::{AtomicUsize, Ordering::*},
    },
};

pub struct SyncPop;

pub struct SyncPush;

/// Array of values that can be taken in parallel.
pub struct Queue<T, D> {
    ptr: *mut T,
    len: AtomicUsize,
    cap: usize,
    marker: PhantomData<D>,
}

pub enum TryReserveError {
    CapacityOverflow,
    AllocError { layout: Layout },
}

use TryReserveError::*;

impl<T, D> Queue<T, D> {
    /// Creates new empty instance of `SyncPush`.
    pub const fn new() -> Self {
        Queue::with_capacity(0)
    }

    /// Creates new empty instance of `SyncPush`.
    pub const fn with_capacity(cap: usize) -> Self {
        Queue {
            ptr: null_mut(),
            len: AtomicUsize::new(0),
            cap: if size_of::<T>() == 0 {
                core::isize::MAX as usize
            } else {
                cap
            },
            marker: PhantomData,
        }
    }

    /// Returns capacity of the queue.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Returns number of elements.
    /// It may grow and shrink concurrently.
    pub fn len(&self) -> usize {
        self.len.load(Relaxed)
    }

    pub fn append(&mut self, values: &mut Vec<T>) {
        let vacant = self.cap - *self.len.get_mut();
        if values.len() >= vacant {
            let add = values.len() - vacant;
            self.reserve(add);
        }

        unsafe {
            values.set_len(0);
            copy_nonoverlapping(
                values.as_ptr(),
                self.ptr.add(*self.len.get_mut()),
                values.len(),
            );
        }
    }

    pub fn push(&mut self, value: T) {
        if *self.len.get_mut() == self.cap {
            self.reserve(1);
        }

        unsafe {
            write(self.ptr.add(*self.len.get_mut()), value);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if *self.len.get_mut() > 0 {
            Some(unsafe { read(self.ptr.add(*self.len.get_mut() - 1)) })
        } else {
            None
        }
    }

    /// Reserives space for `additional` elements.
    pub fn reserve(&mut self, additional: usize) {
        match self.try_reserve(additional) {
            Ok(()) => {}
            Err(CapacityOverflow) => capacity_overflow(),
            Err(AllocError { layout }) => handle_alloc_error(layout),
        }
    }

    fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        if size_of::<T>() == 0 {
            capacity_overflow()
        }

        let cap = self.cap.checked_mul(2).ok_or(CapacityOverflow)?;
        let cap = cap.max(self.cap.checked_add(additional).ok_or(CapacityOverflow)?);
        if cap > core::isize::MAX as usize {
            return Err(CapacityOverflow);
        }

        let layout = Layout::array::<T>(cap).map_err(|_| CapacityOverflow)?;
        let ptr = unsafe { alloc(layout) } as *mut T;

        if ptr.is_null() {
            Err(AllocError { layout })
        } else {
            unsafe {
                copy_nonoverlapping(self.ptr, ptr, self.cap);
            }
            self.ptr = ptr;
            self.cap = cap;

            Ok(())
        }
    }
}

impl<T> Queue<T, SyncPush> {
    /// Tries to push element.
    /// Returns `Ok(())` on success
    /// Otherwise returns `Err(value)`.
    pub fn sync_push(&self, value: T) -> Result<(), T> {
        let len = self.len.fetch_add(1, Acquire);
        if len >= self.cap {
            self.len.store(self.cap, Relaxed);
            Err(value)
        } else {
            unsafe { write(self.ptr.add(len), value) }
            Ok(())
        }
    }
}

impl<T> Queue<T, SyncPop> {
    /// Tries to push element.
    /// Returns `Ok(())` on success
    /// Otherwise returns `Err(value)`.
    pub fn sync_pop(&self) -> Option<T> {
        let len = self.len.fetch_sub(1, Acquire).wrapping_sub(1);
        if len < self.cap {
            self.len.store(0, Relaxed);
            None
        } else {
            Some(unsafe { read(self.ptr.add(len)) })
        }
    }
}

impl<T, D> Drop for Queue<T, D> {
    fn drop(&mut self) {
        let len = *self.len.get_mut();
        unsafe { drop_in_place(slice_from_raw_parts_mut(self.ptr, len)) }
    }
}
