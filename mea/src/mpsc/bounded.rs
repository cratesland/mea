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

//! A bounded multi-producer, single-consumer queue for sending values between asynchronous
//! tasks with backpressure control.

use std::fmt;
use std::future::poll_fn;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use crate::atomicbox::AtomicOptionBox;
use crate::internal::Semaphore;
use crate::mpsc::error::TrySendError;
use crate::mpsc::SendError;
use crate::mpsc::TryRecvError;

/// Creates a bounded mpsc channel for communicating between asynchronous
/// tasks with backpressure.
///
/// A `send` on this channel will wait if the buffer of the channel is full until a
/// `recv` is called on the receiver, which will consume the message and
/// free up space in the buffer.
#[track_caller]
pub fn bounded<T>(buffer: usize) -> (BoundedSender<T>, BoundedReceiver<T>) {
    assert!(buffer > 0, "mpsc bounded channel requires buffer > 0");
    let state = Arc::new(BoundedState {
        senders: AtomicUsize::new(1),
        is_close: AtomicBool::new(false),
        tx_permits: Semaphore::new(buffer),
        rx_task: AtomicOptionBox::none(),
    });
    let (sender, receiver) = std::sync::mpsc::channel();
    let sender = BoundedSender {
        state: state.clone(),
        sender: Some(sender),
    };
    let receiver = BoundedReceiver {
        state: state.clone(),
        receiver: Some(receiver),
    };
    (sender, receiver)
}

struct BoundedState {
    senders: AtomicUsize,
    is_close: AtomicBool,
    tx_permits: Semaphore,
    rx_task: AtomicOptionBox<Waker>,
}

/// Send values to the associated [`BoundedReceiver`].
///
/// Instances are created by the [`bounded`] function.
pub struct BoundedSender<T> {
    state: Arc<BoundedState>,
    sender: Option<std::sync::mpsc::Sender<T>>,
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        self.state.senders.fetch_add(1, Ordering::Release);
        BoundedSender {
            state: self.state.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<T> fmt::Debug for BoundedSender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BoundedSender").finish_non_exhaustive()
    }
}

impl<T> Drop for BoundedSender<T> {
    fn drop(&mut self) {
        // drop the sender; this closes the channel if it is the last sender
        drop(self.sender.take());

        match self.state.senders.fetch_sub(1, Ordering::AcqRel) {
            1 => {
                // If this is the last sender, we need to wake up the receiver so it can
                // observe the disconnected state.
                if let Some(waker) = self.state.rx_task.take(Ordering::Acquire) {
                    waker.wake();
                }
            }
            _ => {
                // there are still other senders left, do nothing
            }
        }
    }
}

impl<T> BoundedSender<T> {
    /// Attempts to send a message to the associated receiver.
    ///
    /// This method will wait if the buffer of the channel is full until a `recv` is called on the
    /// receiver, which will consume the message and free up space in the buffer.
    ///
    /// If the receiver has been dropped, this function returns an error. The error includes
    /// the value passed to `send`.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        if self.state.is_close.load(Ordering::Acquire) {
            return Err(SendError::new(value));
        }
        self.state.tx_permits.acquire(1).await;

        // SAFETY: The sender is guaranteed to be non-null before dropped.
        let sender = self.sender.as_ref().unwrap();
        match sender.send(value) {
            Ok(()) => {}
            Err(err) => {
                self.state.tx_permits.release(1);
                return Err(SendError::new(err.0));
            }
        }

        if let Some(waker) = self.state.rx_task.take(Ordering::Acquire) {
            waker.wake();
        }

        Ok(())
    }

    /// Attempts to send a message to the associated receiver without waiting.
    ///
    /// This method returns the [`Full`] error if the buffer of the channel is full.
    ///
    /// This method returns the [`Disconnected`] error if the channel is currently empty, and there
    /// are no outstanding [receivers].
    ///
    /// [`Full`]: TrySendError::Full
    /// [`Disconnected`]: TrySendError::Disconnected
    /// [receivers]: BoundedReceiver
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mpsc::bounded;
    /// use mea::mpsc::TrySendError;
    /// let (tx, mut rx) = bounded::<i32>(1);
    ///
    /// tx.try_send(1).unwrap();
    /// assert_eq!(tx.try_send(2), Err(TrySendError::Full(2)));
    ///
    /// drop(rx);
    /// assert_eq!(tx.try_send(3), Err(TrySendError::Disconnected(3)));
    /// # }
    /// ```
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if self.state.is_close.load(Ordering::Acquire) {
            return Err(TrySendError::new_disconnected(value));
        }
        match self.state.tx_permits.try_acquire(1) {
            true => {}
            false => return Err(TrySendError::new_full(value)),
        };

        // SAFETY: The sender is guaranteed to be non-null before dropped.
        let sender = self.sender.as_ref().unwrap();
        match sender.send(value) {
            Ok(()) => {}
            Err(err) => {
                self.state.tx_permits.release(1);
                return Err(TrySendError::new_disconnected(err.0));
            }
        }

        if let Some(waker) = self.state.rx_task.take(Ordering::Acquire) {
            waker.wake();
        }

        Ok(())
    }
}

/// Receives values from the associated [`BoundedSender`].
///
/// Instances are created by the [`bounded`] function.
pub struct BoundedReceiver<T> {
    state: Arc<BoundedState>,
    receiver: Option<std::sync::mpsc::Receiver<T>>,
}

/// The only `!Sync` field `receiver` is protected by `&mut self` in `recv` and `try_recv`.
/// That is, `BoundedReceiver` can only be accessed by one thread at a time.
unsafe impl<T: Send> Sync for BoundedReceiver<T> {}

impl<T> fmt::Debug for BoundedReceiver<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BoundedReceiver").finish_non_exhaustive()
    }
}

impl<T> Drop for BoundedReceiver<T> {
    fn drop(&mut self) {
        drop(self.receiver.take());
        self.state.is_close.store(true, Ordering::Release);
        self.state.tx_permits.notify_all();
    }
}

impl<T> BoundedReceiver<T> {
    /// Tries to receive the next value for this receiver and frees up a space in the buffer if
    /// successful.
    ///
    /// This method returns the [`Empty`] error if the channel is currently
    /// empty, but there are still outstanding [senders].
    ///
    /// This method returns the [`Disconnected`] error if the channel is
    /// currently empty, and there are no outstanding [senders].
    ///
    /// [`Empty`]: TryRecvError::Empty
    /// [`Disconnected`]: TryRecvError::Disconnected
    /// [senders]: BoundedSender
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mpsc;
    /// use mea::mpsc::TryRecvError;
    /// let (tx, mut rx) = mpsc::bounded(2);
    ///
    /// tx.send("hello").await.unwrap();
    ///
    /// assert_eq!(Ok("hello"), rx.try_recv());
    /// assert_eq!(Err(TryRecvError::Empty), rx.try_recv());
    ///
    /// tx.send("hello").await.unwrap();
    /// drop(tx);
    ///
    /// assert_eq!(Ok("hello"), rx.try_recv());
    /// assert_eq!(Err(TryRecvError::Disconnected), rx.try_recv());
    /// # }
    /// ```
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        // SAFETY: The receiver is guaranteed to be non-null before dropped.
        let receiver = self.receiver.as_ref().unwrap();
        match receiver.try_recv() {
            Ok(v) => {
                self.state.tx_permits.release(1);
                Ok(v)
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(TryRecvError::Disconnected),
            Err(std::sync::mpsc::TryRecvError::Empty) => Err(TryRecvError::Empty),
        }
    }

    /// Receives the next value for this receiver and frees up a space in the buffer if successful.
    ///
    /// This method returns `None` if the channel has been closed and there are
    /// no remaining messages in the channel's buffer. This indicates that no
    /// further values can ever be received from this `Receiver`. The channel is
    /// closed when all senders have been dropped.
    ///
    /// If there are no messages in the channel's buffer, but the channel has
    /// not yet been closed, this method will sleep until a message is sent or
    /// the channel is closed.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. If `recv` is used as the event in a `select` statement
    /// and some other branch completes first, it is guaranteed that no messages were received
    /// on this channel.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mpsc;
    /// let (tx, mut rx) = mpsc::bounded(1);
    ///
    /// tokio::spawn(async move {
    ///     tx.send("hello").await.unwrap();
    /// });
    ///
    /// assert_eq!(Some("hello"), rx.recv().await);
    /// assert_eq!(None, rx.recv().await);
    /// # }
    /// ```
    ///
    /// Values are buffered if the channel has enough capacity:
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mpsc;
    /// let (tx, mut rx) = mpsc::bounded(2);
    ///
    /// tx.send("hello").await.unwrap();
    /// tx.send("world").await.unwrap();
    ///
    /// assert_eq!(Some("hello"), rx.recv().await);
    /// assert_eq!(Some("world"), rx.recv().await);
    /// # }
    /// ```
    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| self.poll_recv(cx)).await
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.try_recv() {
            Ok(v) => Poll::Ready(Some(v)),
            Err(TryRecvError::Disconnected) => Poll::Ready(None),
            Err(TryRecvError::Empty) => {
                let waker = Some(Box::new(cx.waker().clone()));
                self.state.rx_task.store(waker, Ordering::AcqRel);

                match self.try_recv() {
                    Ok(v) => Poll::Ready(Some(v)),
                    Err(TryRecvError::Disconnected) => Poll::Ready(None),
                    Err(TryRecvError::Empty) => Poll::Pending,
                }
            }
        }
    }
}
