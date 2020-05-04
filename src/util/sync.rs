#[cfg(feature = "parallel")]
pub use self::parallel::*;

#[cfg(not(feature = "parallel"))]
pub use self::orthogonal::*;

#[cfg(not(feature = "parallel"))]
mod orthogonal {
    use std::cell::{RefCell, UnsafeCell};

    pub struct Shared<T> {
        cell: RefCell<T>,
    }

    impl<T> Shared<T> {
        pub fn new(value: T) -> Self {
            Shared {
                cell: RefCell::new(value),
            }
        }

        pub fn lock(&self) -> impl std::ops::DerefMut<Target = T> + '_ {
            self.cell.borrow_mut()
        }

        pub fn get_mut(&mut self) -> &mut T {
            self.cell.get_mut()
        }
    }

    /// Synchronous multithreaded consumable vector.
    pub(crate) struct SyncPop<T> {
        vec: UnsafeCell<Vec<T>>,
    }

    /// Storage can be send if values can be sent.
    unsafe impl<T: Send> Send for SyncPop<T> {}

    impl<T> SyncPop<T> {
        /// Returns mutable reference to inner `Vec`.
        /// This reference must not esacpe public function that takes `&self`.
        /// It also should not be called twice within single public function.
        unsafe fn inner(&self) -> &mut Vec<T> {
            &mut *self.vec.get()
        }

        // /// Create storage without values.
        // pub fn new() -> Self {
        //     SyncPop {
        //         vec: UnsafeCell::new(Vec::new()),
        //     }
        // }

        /// Create storage with values.
        pub fn from_iter(values: impl IntoIterator<Item = T>) -> Self {
            SyncPop {
                vec: UnsafeCell::new(values.into_iter().collect()),
            }
        }

        /// Pops next value from storage and returns it.
        /// Returns `None` if all values have been already taken.
        pub fn pop(&self) -> Option<T> {
            unsafe {
                // Reference doesn't escape this function
                self.inner().pop()
            }
        }

        /// Pops next value from storage and returns it.
        /// Returns `None` if all values have been already taken.
        pub fn pop_mut(&mut self) -> Option<T> {
            unsafe {
                // Reference doesn't escape this function
                self.inner().pop()
            }
        }

        // /// Returns excess capacity.
        // pub fn excess(&mut self) -> usize {
        //     unsafe {
        //         // Reference doesn't escape this function
        //         let vec = self.inner();
        //         vec.capacity() - vec.len()
        //     }
        // }

        /// Fills storage with values.
        pub fn extend(&mut self, values: impl IntoIterator<Item = T>) {
            unsafe {
                // Reference doesn't escape this function
                self.inner().extend(values)
            }
        }

        /// Add value into storage
        pub fn push(&mut self, value: T) {
            unsafe {
                // Reference doesn't escape this function
                self.inner().push(value);
            }
        }

        /// Reserive additional capacity.
        pub fn reserve(&mut self, additional: usize) {
            unsafe {
                // Reference doesn't escape this function
                self.inner().reserve(additional)
            }
        }
    }

    /// Multi-consumer storage.
    pub(crate) struct SyncPush<T> {
        vec: UnsafeCell<Vec<T>>,
    }

    /// Storage can be send if values can be sent.
    unsafe impl<T: Send> Send for SyncPush<T> {}

    impl<T> SyncPush<T> {
        /// Returns mutable reference to inner `Vec`.
        /// This reference must not esacpe public function that takes `&self`.
        /// It also should not be called twice within single public function.
        unsafe fn inner(&self) -> &mut Vec<T> {
            &mut *self.vec.get()
        }

        /// Create storage with specified capacity.
        pub fn new(capacity: usize) -> Self {
            SyncPush {
                vec: UnsafeCell::new(Vec::with_capacity(capacity)),
            }
        }

        /// Pushes value into storage.
        /// Returs it back if storage is full.
        pub fn push(&self, value: T) -> usize {
            unsafe {
                // Reference doesn't escape this function
                let inner = self.inner();
                let index = inner.len();
                inner.push(value);
                index
            }
        }

        /// Drain values.
        pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
            unsafe {
                // Reference escapes this function.
                // But function borrows `self` mutably.
                // So `self.inner()` won't be called again
                // until returned iterator is dropped
                self.inner().drain(..)
            }
        }

        // /// Reserive additional capacity.
        // pub fn reserve(&mut self, additional: usize) {
        //     unsafe {
        //         // Reference doesn't escape this function
        //         self.inner().reserve(additional);
        //     }
        // }
    }
}

#[cfg(feature = "parallel")]
mod parallel {
    use {
        parking_lot::Mutex,
        std::sync::atomic::{AtomicUsize, Ordering},
    };

    pub use parking_lot::Mutex as Shared;

    /// Synchronous multithreaded consumable vector.
    pub(crate) struct SyncPop<T> {
        ptr: *mut T,
        len: AtomicUsize,
        cap: usize,
    }

    /// Storage can be send if values can be sent.
    unsafe impl<T: Send> Send for SyncPop<T> {}

    /// Storage can be shared with multiple threads if values can be sent.
    /// As storage doesn't work with references to values
    /// so `T: Sync` isn't necessary.
    unsafe impl<T: Send> Sync for SyncPop<T> {}

    impl<T> SyncPop<T> {
        /// Create storage with values.
        pub fn from_iter(values: impl IntoIterator<Item = T>) -> Self {
            let mut vec: Vec<_> = values.into_iter().collect();
            let ptr = vec.as_mut_ptr();
            let len = vec.len();
            let cap = vec.capacity();
            std::mem::forget(vec);
            SyncPop {
                ptr,
                len: AtomicUsize::new(len),
                cap,
            }
        }

        /// Pops next value from storage and returns it.
        /// Returns `None` if all values have been already taken.
        pub fn pop(&self) -> Option<T> {
            // Acquire reading index.
            let index = self.len.fetch_sub(1, Ordering::Acquire).wrapping_sub(1);
            if index < self.cap {
                unsafe {
                    // Index checked.
                    // No other thread can access this index.
                    Some(std::ptr::read(self.ptr.add(index)))
                }
            } else {
                // Prevent wrap around.
                self.len.fetch_add(1, Ordering::Release);
                None
            }
        }

        /// Pops next value from storage and returns it.
        /// Returns `None` if all values have been already taken.
        pub fn pop_mut(&mut self) -> Option<T> {
            // Acquire reading index.
            let len = self.len.get_mut();
            if *len > 0 {
                *len -= 1;
                unsafe {
                    // Index checked.
                    // No other thread can access this index.
                    Some(std::ptr::read(self.ptr.add(*len)))
                }
            } else {
                None
            }
        }

        /// Fills storage with values.
        pub fn extend(&mut self, values: impl IntoIterator<Item = T>) {
            let mut iter = values.into_iter();
            while let Some(value) = iter.next() {
                if *self.len.get_mut() == self.cap {
                    let (lower, _) = iter.size_hint();
                    self.reserve(lower.saturating_add(1));
                }
                unsafe {
                    std::ptr::write(self.ptr.add(*self.len.get_mut()), value);
                    *self.len.get_mut() += 1;
                }
            }
        }

        /// Add value into storage
        pub fn push(&mut self, value: T) {
            if *self.len.get_mut() == self.cap {
                self.reserve(1);
            }
            unsafe {
                std::ptr::write(self.ptr.add(*self.len.get_mut()), value);
                *self.len.get_mut() += 1;
            }
        }

        /// Reserive additional capacity.
        pub fn reserve(&mut self, additional: usize) {
            if additional > 0 {
                let mut vec = Vec::with_capacity(self.cap + additional);
                let mut ptr = vec.as_mut_ptr();
                let mut cap = vec.capacity();

                debug_assert!(*self.len.get_mut() <= cap);
                std::mem::forget(vec);

                unsafe {
                    std::ptr::copy_nonoverlapping(self.ptr, ptr, *self.len.get_mut());
                    std::mem::swap(&mut self.ptr, &mut ptr);
                    std::mem::swap(&mut self.cap, &mut cap);
                    Vec::from_raw_parts(ptr, 0, cap);
                }
            }
        }
    }

    impl<T> Drop for SyncPop<T> {
        fn drop(&mut self) {
            unsafe {
                drop(Vec::from_raw_parts(self.ptr, *self.len.get_mut(), self.cap));
            }
        }
    }

    /// Multi-consumer storage.
    pub(crate) struct SyncPush<T> {
        ptr: *mut T,
        len: AtomicUsize,
        cap: usize,
        slow: Mutex<Vec<T>>,
    }

    /// Storage can be send if values can be sent.
    unsafe impl<T: Send> Send for SyncPush<T> {}

    /// Storage can be shared with multiple threads if values can be sent.
    /// As storage doesn't work with references to values
    /// so `T: Sync` isn't necessary.
    unsafe impl<T: Send> Sync for SyncPush<T> {}

    impl<T> SyncPush<T> {
        /// Create storage with specified capacity.
        pub fn new(capacity: usize) -> Self {
            let mut vec = Vec::with_capacity(capacity);
            let ptr = vec.as_mut_ptr();
            let cap = vec.capacity();
            std::mem::forget(vec);
            SyncPush {
                ptr,
                len: AtomicUsize::new(0),
                cap,
                slow: Mutex::new(Vec::new()),
            }
        }

        /// Pushes value into storage.
        pub fn push(&self, value: T) -> usize {
            let index = self.len.fetch_add(1, Ordering::Acquire);
            if index < self.cap {
                unsafe {
                    // Index checked.
                    // No other thread can access this index.
                    std::ptr::write(self.ptr.add(index), value);
                }
                index
            } else {
                // Prevent counter overflow
                self.len.fetch_sub(1, Ordering::Release);
                let mut lock = self.slow.lock();
                let index = lock.len();
                lock.push(value);
                drop(lock);
                index + self.cap
            }
        }

        /// Drain values.
        pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
            struct Drain<'a, T> {
                ptr: *mut T,
                len: &'a mut usize,
            }

            impl<'a, T> Iterator for Drain<'a, T> {
                type Item = T;

                fn next(&mut self) -> Option<T> {
                    if *self.len > 0 {
                        *self.len -= 1;
                        Some(unsafe {
                            // Mutable access to cell.
                            // Whole slice is init and traversed only once.
                            std::ptr::read(self.ptr.add(*self.len))
                        })
                    } else {
                        None
                    }
                }
            }

            self.slow.get_mut().drain(..).chain(Drain {
                ptr: self.ptr,
                len: self.len.get_mut(),
            })
        }
    }

    impl<T> Drop for SyncPush<T> {
        fn drop(&mut self) {
            unsafe {
                drop(Vec::from_raw_parts(self.ptr, *self.len.get_mut(), self.cap));
            }
        }
    }
}
