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

//! A multi-producer, single-consumer queue for sending values between asynchronous tasks.

use std::fmt;
use std::future::poll_fn;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use self::mpsc_queue::*;
use crate::atomicbox::AtomicOptionBox;

#[cfg(test)]
mod tests;

/// Creates an unbounded mpsc channel for communicating between asynchronous
/// tasks without backpressure.
///
/// A `send` on this channel will always succeed as long as the receiver is alive.
/// If the receiver falls behind, messages will be arbitrarily buffered.
///
/// Note that the amount of available system memory is an implicit bound to
/// the channel. Using an `unbounded` channel has the ability of causing the
/// process to run out of memory. In this case, the process will be aborted.
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let queue = make_queue();
    let state = Arc::new(UnboundedState {
        mq: queue,
        senders: AtomicUsize::new(1),
        disconnected: AtomicBool::new(false),
        rx_task: AtomicOptionBox::none(),
    });
    let sender = UnboundedSender {
        state: state.clone(),
    };
    let receiver = UnboundedReceiver {
        state: state.clone(),
    };
    (sender, receiver)
}

struct UnboundedState<T> {
    mq: Queue<T>,
    senders: AtomicUsize,
    disconnected: AtomicBool,
    rx_task: AtomicOptionBox<Waker>,
}

/// Send values to the associated [`UnboundedReceiver`].
///
/// Instances are created by the [`unbounded`] function.
pub struct UnboundedSender<T> {
    state: Arc<UnboundedState<T>>,
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        self.state.senders.fetch_add(1, Ordering::Release);
        UnboundedSender {
            state: self.state.clone(),
        }
    }
}

impl<T> fmt::Debug for UnboundedSender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnboundedSender").finish_non_exhaustive()
    }
}

impl<T> Drop for UnboundedSender<T> {
    fn drop(&mut self) {
        match self.state.senders.fetch_sub(1, Ordering::AcqRel) {
            1 => {
                // this is the last sender, we can disconnect the channel
                self.state.disconnected.store(true, Ordering::Relaxed);
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

impl<T> UnboundedSender<T> {
    /// Attempts to send a message without blocking.
    ///
    /// This method is not marked async because sending a message to an unbounded channel
    /// never requires any form of waiting. Because of this, the `send` method can be
    /// used in both synchronous and asynchronous code without problems.
    ///
    /// If the receiver has been dropped, this function returns an error. The error includes
    /// the value passed to `send`.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if self.state.disconnected.load(Ordering::Relaxed) {
            return Err(SendError(value));
        }

        self.state.mq.push(value);
        if let Some(waker) = self.state.rx_task.take(Ordering::Acquire) {
            waker.wake();
        }

        Ok(())
    }
}

/// An error returned when trying to send on a closed channel. Returned from
/// [`UnboundedSender::send`] if the corresponding [`UnboundedReceiver`] has
/// already been dropped.
///
/// The message that could not be sent can be retrieved again with
/// [`SendError::into_inner`].
pub struct SendError<T>(T);

impl<T> SendError<T> {
    /// Get a reference to the message that failed to be sent.
    pub fn as_inner(&self) -> &T {
        &self.0
    }

    /// Consumes the error and returns the message that failed to be sent.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "sending on a closed channel".fmt(f)
    }
}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendError<{}>(..)", stringify!(T))
    }
}

impl<T> std::error::Error for SendError<T> {}

/// Receive values from the associated [`UnboundedSender`].
///
/// Instances are created by the [`unbounded`] function.
pub struct UnboundedReceiver<T> {
    state: Arc<UnboundedState<T>>,
}

impl<T> fmt::Debug for UnboundedReceiver<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnboundedReceiver")
            .finish_non_exhaustive()
    }
}

impl<T> Drop for UnboundedReceiver<T> {
    fn drop(&mut self) {
        self.state.disconnected.store(true, Ordering::Relaxed);
        self.state.rx_task.take(Ordering::Acquire);
    }
}

impl<T> UnboundedReceiver<T> {
    /// Tries to receive the next value for this receiver.
    ///
    /// This method returns the [`Empty`] error if the channel is currently
    /// empty, but there are still outstanding [senders].
    ///
    /// This method returns the [`Disconnected`] error if the channel is
    /// currently empty, and there are no outstanding [senders].
    ///
    /// [`Empty`]: TryRecvError::Empty
    /// [`Disconnected`]: TryRecvError::Disconnected
    /// [senders]: UnboundedSender
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mpsc;
    /// use mea::mpsc::TryRecvError;
    /// let (tx, mut rx) = mpsc::unbounded();
    ///
    /// tx.send("hello").unwrap();
    ///
    /// assert_eq!(Ok("hello"), rx.try_recv());
    /// assert_eq!(Err(TryRecvError::Empty), rx.try_recv());
    ///
    /// tx.send("hello").unwrap();
    /// drop(tx);
    ///
    /// assert_eq!(Ok("hello"), rx.try_recv());
    /// assert_eq!(Err(TryRecvError::Disconnected), rx.try_recv());
    /// # }
    /// ```
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        // SAFETY: &mut self indicates that we are the single consumer
        match unsafe { self.state.mq.pop() } {
            Some(v) => Ok(v),
            None => {
                if self.state.disconnected.load(Ordering::Relaxed) {
                    Err(TryRecvError::Disconnected)
                } else {
                    Err(TryRecvError::Empty)
                }
            }
        }
    }

    /// Receives the next value for this receiver.
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
    /// let (tx, mut rx) = mpsc::unbounded();
    ///
    /// tokio::spawn(async move {
    ///     tx.send("hello").unwrap();
    /// });
    ///
    /// assert_eq!(Some("hello"), rx.recv().await);
    /// assert_eq!(None, rx.recv().await);
    /// # }
    /// ```
    ///
    /// Values are buffered:
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mpsc;
    /// let (tx, mut rx) = mpsc::unbounded();
    ///
    /// tx.send("hello").unwrap();
    /// tx.send("world").unwrap();
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

/// Error returned by `try_recv`.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TryRecvError {
    /// This channel is currently empty, but the sender(s) have not yet disconnected, so data may
    /// yet become available.
    Empty,
    /// The sender has become disconnected, and there will never be any more data received on it.
    Disconnected,
}

impl fmt::Display for TryRecvError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TryRecvError::Empty => "receiving on an empty channel".fmt(fmt),
            TryRecvError::Disconnected => "receiving on a closed channel".fmt(fmt),
        }
    }
}

impl std::error::Error for TryRecvError {}

// http://www.1024cores.net/home/lock-free-algorithms/queues/non-intrusive-mpsc-node-based-queue
mod mpsc_queue {
    use std::cell::UnsafeCell;
    use std::sync::atomic::AtomicPtr;
    use std::sync::atomic::Ordering;

    struct Node<T> {
        next: AtomicPtr<Node<T>>,
        value: Option<T>,
    }

    fn make_node<T>(v: Option<T>) -> *mut Node<T> {
        Box::into_raw(Box::new(Node {
            next: AtomicPtr::new(std::ptr::null_mut()),
            value: v,
        }))
    }

    pub struct Queue<T> {
        head: AtomicPtr<Node<T>>,
        tail: UnsafeCell<*mut Node<T>>,
    }

    unsafe impl<T: Send> Send for Queue<T> {}
    unsafe impl<T: Send> Sync for Queue<T> {}

    impl<T> Drop for Queue<T> {
        fn drop(&mut self) {
            let mut tail = *self.tail.get_mut();
            while !tail.is_null() {
                let next = unsafe { (*tail).next.load(Ordering::Relaxed) };
                drop(unsafe { Box::from_raw(tail) });
                tail = next;
            }
        }
    }

    pub fn make_queue<T>() -> Queue<T> {
        let stub = make_node(None);
        Queue {
            head: AtomicPtr::new(stub),
            tail: UnsafeCell::new(stub),
        }
    }

    impl<T> Queue<T> {
        pub fn push(&self, v: T) {
            let node = make_node(Some(v));
            // serialization-point wrt producers, acquire-release
            let prev = self.head.swap(node, Ordering::AcqRel);
            // serialization-point wrt consumer, release
            unsafe { (*prev).next.store(node, Ordering::Release) };
        }

        /// # Safety
        ///
        /// This function must be called by a single consumer thread.
        pub unsafe fn pop(&self) -> Option<T> {
            let tail = *self.tail.get();

            // serialization-point wrt producers, acquire
            let next = (*tail).next.load(Ordering::Acquire);

            if next.is_null() {
                None
            } else {
                *self.tail.get() = next;
                drop(Box::from_raw(tail));
                Some((*next).value.take().unwrap())
            }
        }
    }
}
