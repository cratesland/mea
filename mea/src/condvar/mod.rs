use crate::internal::Mutex;
use crate::mutex::MutexGuard;
use slab::Slab;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

struct WakerSet {
    entries: Slab<Option<Waker>>,
    notifiable: usize,
}

impl WakerSet {
    fn insert(&mut self, cx: &Context<'_>) -> usize {
        let key = self.entries.insert(Some(cx.waker().clone()));
        self.notifiable += 1;
        key
    }

    fn remove_if_notified(&mut self, key: usize, cx: &Context<'_>) -> bool {
        match &mut self.entries[key] {
            None => {
                self.entries.remove(key);
                true
            }
            Some(w) => {
                // We were never woken, so update instead
                if !w.will_wake(cx.waker()) {
                    *w = cx.waker().clone();
                }
                false
            }
        }
    }

    fn notify_all(&mut self) -> bool {
        if self.notifiable > 0 {
            let mut notified = false;
            for (_, opt_waker) in self.entries.iter_mut() {
                if let Some(w) = opt_waker.take() {
                    w.wake();
                    self.notifiable -= 1;
                    notified = true;
                }
            }
            assert_eq!(self.notifiable, 0);
            notified
        } else {
            false
        }
    }

    fn notify_one(&mut self) -> bool {
        if self.notifiable > 0 {
            for (_, opt_waker) in self.entries.iter_mut() {
                if let Some(w) = opt_waker.take() {
                    w.wake();
                    self.notifiable -= 1;
                    return true;
                }
            }
        }
        false
    }

    fn cancel(&mut self, key: usize) -> bool {
        match self.entries.remove(key) {
            Some(_) => {
                self.notifiable -= 1;
                false
            }
            None => self.notify_one(),
        }
    }
}

pub struct Condvar {
    waker_set: Mutex<WakerSet>,
}

impl Condvar {
    pub fn new() -> Condvar {
        Condvar {
            waker_set: Mutex::new(WakerSet {
                entries: Slab::new(),
                notifiable: 0,
            }),
        }
    }

    pub fn notify_one(&self) {
        let mut waker_set = self.waker_set.lock();
        waker_set.notify_one();
    }

    pub fn notify_all(&self) {
        let mut waker_set = self.waker_set.lock();
        waker_set.notify_all();
    }

    pub async fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let mutex = MutexGuard::source(&guard);

        let fut = AwaitNotify {
            cond: self,
            guard: Some(guard),
            key: None,
        };
        fut.await;

        mutex.lock().await
    }
}

struct AwaitNotify<'a, 'b, T> {
    cond: &'a Condvar,
    guard: Option<MutexGuard<'b, T>>,
    key: Option<usize>,
}

impl<'a, 'b, T> Future for AwaitNotify<'a, 'b, T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.guard.take() {
            Some(_) => {
                self.key = {
                    let mut waker_set = self.cond.waker_set.lock();
                    Some(waker_set.insert(cx))
                };
                // the guard is dropped when we return, which frees the lock
                Poll::Pending
            }
            None => {
                if let Some(key) = self.key {
                    let mut waker_set = self.cond.waker_set.lock();
                    if waker_set.remove_if_notified(key, cx) {
                        self.key = None;
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                } else {
                    // This should only happen if it is polled twice after receiving a notification
                    Poll::Ready(())
                }
            }
        }
    }
}

impl<'a, 'b, T> Drop for AwaitNotify<'a, 'b, T> {
    fn drop(&mut self) {
        let mut waker_set = self.cond.waker_set.lock();
        if let Some(key) = self.key {
            waker_set.cancel(key);
        }
    }
}
