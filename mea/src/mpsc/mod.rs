//!

use crate::internal::Mutex;
use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

#[cfg(test)]
mod tests;

///
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let state = Arc::new(Mutex::new(UnboundedState {
        buffer: VecDeque::new(),
        rx_task: None,
    }));

    (
        UnboundedSender {
            state: state.clone(),
        },
        UnboundedReceiver {
            state: state.clone(),
        },
    )
}

///
#[derive(Clone)]
pub struct UnboundedSender<T> {
    state: Arc<Mutex<UnboundedState<T>>>,
}

impl<T> UnboundedSender<T> {
    ///
    pub fn send(&self, value: T) {
        let mut state = self.state.lock();
        state.buffer.push_back(value);
        if let Some(waker) = state.rx_task.take() {
            waker.wake();
        }
    }
}

///
pub struct UnboundedReceiver<T> {
    state: Arc<Mutex<UnboundedState<T>>>,
}

impl<T> UnboundedReceiver<T> {
    ///
    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| self.poll_recv(cx)).await
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut state = self.state.lock();
        if let Some(value) = state.buffer.pop_front() {
            Poll::Ready(Some(value))
        } else {
            state.rx_task = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

struct UnboundedState<T> {
    buffer: VecDeque<T>,
    rx_task: Option<Waker>,
}
