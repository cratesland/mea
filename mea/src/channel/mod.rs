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

//! A multi-producer, multi-consumer channel.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::pin;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::ready;
use std::task::Context;
use std::task::Poll;

use futures_core::Stream;

use crate::primitives::condvar::Condvar;
use crate::primitives::mutex::Mutex;

#[cfg(test)]
mod tests;

/// Create a channel with no maximum capacity.
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared::new(None));
    let sender = Sender {
        shared: shared.clone(),
    };
    let receiver = Receiver { shared };
    (sender, receiver)
}

/// Create a channel with a maximum capacity.
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
        self.receiver_wait.notify_all();
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
        self.capacity.is_some_and(|cap| self.buffer.len() >= cap)
    }

    fn push_back(&mut self, item: T) {
        self.buffer.push_back(item);
    }

    fn pop_front(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }
}

/// An error that may be emitted when attempting to send a value into a channel on a sender when
/// the channel is full or all receivers are dropped.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TrySendError<T> {
    /// The channel the message is sent on has a finite capacity and was full when attempt to send.
    Full(T),
    /// All channel receivers were dropped and so the message has nobody to receive it.
    Disconnected(T),
}

impl<T> fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TrySendError::Full(..) => write!(f, "Full(..)"),
            TrySendError::Disconnected(..) => write!(f, "Disconnected(..)"),
        }
    }
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrySendError::Full(..) => write!(f, "sending on a closed channel"),
            TrySendError::Disconnected(..) => write!(f, "sending on a closed channel"),
        }
    }
}

impl<T> std::error::Error for TrySendError<T> {}

/// An error that may be emitted when attempting to send a value into a channel on a sender when
/// all receivers are dropped.
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

/// A transmitting end of a channel.
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
    /// Attempt to send a value into the channel. If the channel is bounded and full, or all
    /// receivers have been dropped, [`TrySendError`] will be returned.
    pub fn try_send(&self, item: T) -> Result<(), TrySendError<T>> {
        pollster::block_on(self.do_try_send(item))
    }

    async fn do_try_send(&self, item: T) -> Result<(), TrySendError<T>> {
        let mut channel = self.shared.channel.lock().await;

        if self.shared.is_disconnected() {
            return Err(TrySendError::Disconnected(item));
        }

        if channel.is_full() {
            return Err(TrySendError::Full(item));
        }

        channel.push_back(item);
        drop(channel);

        self.shared.receiver_wait.notify_one();
        self.shared.sender_wait.notify_one();
        Ok(())
    }

    /// Asynchronously send a value into the channel, returning [`SendError`] if all receivers have
    /// been dropped. If the channel is bounded and is full, the returned future will yield to
    /// the async runtime.
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

/// An error that may be emitted when attempting to fetch a value on a receiver when there are no
/// messages in the channel. If there are no messages in the channel and all senders are dropped,
/// then `TryRecvError::Disconnected` will be returned.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TryRecvError {
    /// The channel was empty when attempt to receive.
    Empty,
    /// All senders were dropped and no messages are waiting in the channel, so no further messages
    /// can be received.
    Disconnected,
}

impl fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TryRecvError::Empty => write!(f, "receiving on an empty channel"),
            TryRecvError::Disconnected => write!(f, "receiving on a closed channel"),
        }
    }
}

impl std::error::Error for TryRecvError {}

/// An error that may be emitted when attempting to wait for a value on a receiver when all senders
/// are dropped and there are no more messages in the channel.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct RecvError(());

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "receiving on a closed channel")
    }
}

impl std::error::Error for RecvError {}

/// The receiving end of a channel.
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
    /// Attempt to fetch an incoming value from the channel associated with this receiver,
    /// returning [`TryRecvError`] if the channel is empty or if all senders have been dropped.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        pollster::block_on(self.do_try_recv())
    }

    async fn do_try_recv(&self) -> Result<T, TryRecvError> {
        let mut channel = self.shared.channel.lock().await;

        if let Some(item) = channel.pop_front() {
            self.shared.sender_wait.notify_one();
            return Ok(item);
        }

        if self.shared.is_disconnected() {
            return Err(TryRecvError::Disconnected);
        }

        Err(TryRecvError::Empty)
    }

    /// Asynchronously receive a value from the channel, returning [`RecvError`] if all senders have
    /// been dropped. If the channel is empty, the returned future will yield to the async
    /// runtime.
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

    async fn recv_owned(self) -> StreamContext<T> {
        let result = self.recv().await;
        StreamContext {
            result,
            receiver: self,
        }
    }

    /// Convert this receiver into a stream that allows asynchronously receiving messages from the
    /// channel.
    pub fn into_stream(self) -> impl Stream<Item = T> {
        RecvStream {
            future: self.recv_owned(),
            function: Receiver::recv_owned,
        }
    }
}

struct StreamContext<T> {
    result: Result<T, RecvError>,
    receiver: Receiver<T>,
}

/// A stream which allows asynchronously receiving messages.
#[pin_project::pin_project]
pub struct RecvStream<T, Fut> {
    #[pin]
    future: Fut,
    function: fn(Receiver<T>) -> Fut,
}

impl<T, Fut> Stream for RecvStream<T, Fut>
where
    Fut: Future<Output = StreamContext<T>>,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let StreamContext { result, receiver } = ready!(this.future.as_mut().poll(cx));
        match result {
            Ok(next) => {
                this.future.set((this.function)(receiver));
                Poll::Ready(Some(next))
            }
            Err(RecvError(())) => Poll::Ready(None),
        }
    }
}
