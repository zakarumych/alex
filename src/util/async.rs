use {
    super::mutex::Mutex,
    alloc::collections::VecDeque,
    core::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicI64, Ordering::*},
        task::{Context, Poll, Waker},
    },
};

enum Kind {
    Shared,
    Mutable,
}

pub struct AsyncLock {
    state: AtomicI64,
    wakers: Mutex<VecDeque<(Waker, Kind)>>,
}

pub struct SharedGuard<'a> {
    lock: &'a AsyncLock,
}

impl<'a> Drop for SharedGuard<'a> {
    fn drop(&mut self) {
        let state = self.lock.state.fetch_sub(1, Release);
        debug_assert!(state > 0);
        if state == 1 {
            let mut guard = self.lock.wakers.lock();
            while let Some((waker, kind)) = guard.pop_front() {
                waker.wake();
            }
        }
    }
}

pub struct MutableGuard<'a> {
    lock: &'a AsyncLock,
}

impl<'a> Drop for MutableGuard<'a> {
    fn drop(&mut self) {
        debug_assert!(self.lock.state.load(Relaxed) < 0);
        self.lock.state.store(0, Release);
        let mut guard = self.lock.wakers.lock();
        while let Some((waker, kind)) = guard.pop_front() {
            waker.wake();
        }
    }
}

impl AsyncLock {
    pub fn new() -> Self {
        AsyncLock {
            state: AtomicI64::new(0),
            wakers: Mutex::new(VecDeque::new()),
        }
    }

    pub fn try_lock_shared(&self) -> Option<SharedGuard<'_>> {
        let state = self.state.fetch_add(1, Acquire);
        if state >= 0 {
            Some(SharedGuard { lock: self })
        } else {
            None
        }
    }

    pub fn try_lock_mutable(&self) -> Option<MutableGuard<'_>> {
        if self
            .state
            .compare_exchange(0, i64::MIN, Acquire, Relaxed)
            .is_ok()
        {
            Some(MutableGuard { lock: self })
        } else {
            None
        }
    }

    pub async fn lock_shared<'a>(&'a self) -> SharedGuard<'a> {
        SharedLockFuture { lock: self }.await
    }

    pub async fn lock_mutable<'a>(&'a self) -> MutableGuard<'a> {
        MutableLockFuture { lock: self }.await
    }
}

struct SharedLockFuture<'a> {
    lock: &'a AsyncLock,
}

impl<'a> Future for SharedLockFuture<'a> {
    type Output = SharedGuard<'a>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<SharedGuard<'a>> {
        match self.lock.try_lock_shared() {
            Some(guard) => Poll::Ready(guard),
            None => {
                self.lock
                    .wakers
                    .lock()
                    .push_back((ctx.waker().clone(), Kind::Shared));
                Poll::Pending
            }
        }
    }
}

struct MutableLockFuture<'a> {
    lock: &'a AsyncLock,
}

impl<'a> Future for MutableLockFuture<'a> {
    type Output = MutableGuard<'a>;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<MutableGuard<'a>> {
        match self.lock.try_lock_mutable() {
            Some(guard) => Poll::Ready(guard),
            None => {
                self.lock
                    .wakers
                    .lock()
                    .push_back((ctx.waker().clone(), Kind::Shared));
                Poll::Pending
            }
        }
    }
}
