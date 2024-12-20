// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use futures_core::Stream;

use crate::primitives::condvar::Condvar;
use crate::primitives::mutex::Mutex;

#[cfg(test)]
mod tests;

pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared::new(None));
    let sender = Sender {
        shared: shared.clone(),
    };
    let receiver = Receiver { shared };
    (sender, receiver)
}

pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared::new(Some(capacity)));
    let sender = Sender {
        shared: shared.clone(),
    };
    let receiver = Receiver { shared };
    (sender, receiver)
}

struct Shared<T> {
    channel: Mutex<Channel<T>>,
    sender_wait: Condvar,
    receiver_wait: Condvar,
    disconnected: AtomicBool,
    sender_cnt: AtomicUsize,
    receiver_cnt: AtomicUsize,
}

impl<T> Shared<T> {
    fn new(capacity: Option<usize>) -> Self {
        let buffer = VecDeque::with_capacity(capacity.unwrap_or(0));
        Self {
            channel: Mutex::new(Channel { buffer, capacity }),
            sender_wait: Condvar::new(),
            receiver_wait: Condvar::new(),
            disconnected: AtomicBool::new(false),
            sender_cnt: AtomicUsize::new(1),
            receiver_cnt: AtomicUsize::new(1),
        }
    }

    fn disconnect(&self) {
        self.disconnected.store(true, Ordering::Relaxed);
        self.sender_wait.notify_all();
    }

    fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::SeqCst)
    }
}

struct Channel<T> {
    buffer: VecDeque<T>,
    capacity: Option<usize>,
}

impl<T> Channel<T> {
    fn is_full(&self) -> bool {
        self.capacity.map_or(false, |cap| self.buffer.len() >= cap)
    }

    fn push_back(&mut self, item: T) {
        self.buffer.push_back(item);
    }

    fn pop_front(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct SendError<T>(pub T);

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SendError").finish_non_exhaustive()
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sending on a closed channel")
    }
}

impl<T> std::error::Error for SendError<T> {}

pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.sender_cnt.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.shared.sender_cnt.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.shared.disconnect();
        }
    }
}

impl<T> Sender<T> {
    pub async fn send(&self, item: T) -> Result<(), SendError<T>> {
        let mut channel = self.shared.channel.lock().await;
        if self.shared.is_disconnected() {
            return Err(SendError(item));
        }

        while channel.is_full() && !self.shared.is_disconnected() {
            channel = self.shared.sender_wait.wait(channel).await;
        }

        if self.shared.is_disconnected() {
            return Err(SendError(item));
        }

        channel.push_back(item);
        drop(channel);

        self.shared.receiver_wait.notify_one();
        self.shared.sender_wait.notify_one();
        Ok(())
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct RecvError(());

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "receiving on a closed channel")
    }
}

impl std::error::Error for RecvError {}

pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.shared.receiver_cnt.fetch_add(1, Ordering::Relaxed);
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if self.shared.receiver_cnt.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.shared.disconnect();
        }
    }
}

impl<T> Receiver<T> {
    pub async fn recv(&self) -> Result<T, RecvError> {
        let mut channel = self.shared.channel.lock().await;
        loop {
            if let Some(item) = channel.pop_front() {
                self.shared.sender_wait.notify_one();
                return Ok(item);
            }

            if self.shared.is_disconnected() {
                return Err(RecvError(()));
            }

            channel = self.shared.receiver_wait.wait(channel).await;
        }
    }

    pub fn into_stream(self) -> impl Stream<Item = T> {
        async_stream::stream! {
            while let Ok(item) = self.recv().await {
                yield item;
            }
        }
    }
}
