use futures::FutureExt;
use tokio::time::Sleep;

use crate::priority::Priority;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll},
};

use super::runtime_handle::CommonRuntimeHandle;

enum State {
    Common,
    Backoff(Pin<Box<Sleep>>),
}
impl State {
    fn unwrap_backoff(&mut self) -> &mut Pin<Box<Sleep>> {
        match self {
            State::Backoff(sleep) => sleep,
            _ => panic!("unwrap_backoff failed"),
        }
    }
}

#[pin_project::pin_project]
pub struct PendingFuture<F: Future + Send + 'static> {
    #[pin]
    future: F,
    /// priority of this future
    handle: CommonRuntimeHandle,
    /// count of inner future pending
    pub pend_cnt: Arc<AtomicU32>, // track the pending count for test
    pub inserted_pend_cnt: Arc<AtomicU32>,
    adding_up_time: u64,
    state: State,
}

impl<F> PendingFuture<F>
where
    F: Future + Send + 'static,
{
    pub fn new(handle: CommonRuntimeHandle, future: F) -> Self {
        Self {
            future,
            pend_cnt: Default::default(),
            handle,
            adding_up_time: Default::default(),
            inserted_pend_cnt: Default::default(),
            state: State::Common,
        }
    }
}

impl<F> Future for PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    type Output = F::Output;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.state {
            State::Common => {}
            State::Backoff(ref mut sleep) => match sleep.poll_unpin(cx) {
                Poll::Ready(_) => {
                    *this.state = State::Common;
                }
                Poll::Pending => return Poll::Pending,
            },
        };

        let pend_period = this.handle.0.fixed_interval_time_ms();
        let pendcnt = this.pend_cnt.clone();
        let inserted_pendcnt = this.inserted_pend_cnt.clone();
        // let adding_up_time = this.adding_up_time.clone();

        let inter = 10;
        if let Some(pend_period) = pend_period {
            if *this.adding_up_time > inter {
                // adding_up_time.fetch_sub(13, Ordering::Release);
                let time = *this.adding_up_time / inter;
                *this.adding_up_time -= time * inter;
                inserted_pendcnt.fetch_add(1, Ordering::Release);
                // Register the current task to be woken when the pending period is reached.
                // let waker = cx.waker().clone();
                // let a = tokio::task::yield_now();
                println!(
                    "will sleep {} * {} / 2 + 2 = {} ms",
                    pend_period,
                    time,
                    pend_period * time / 2 + 2
                );
                *this.state = State::Backoff(Box::pin(tokio::time::sleep(
                    std::time::Duration::from_millis(pend_period * time / 2 + 2),
                )));
                match this.state.unwrap_backoff().poll_unpin(cx) {
                    Poll::Ready(_) => {
                        *this.state = State::Common;
                        cx.waker().clone().wake();
                        return Poll::Pending;
                    }
                    Poll::Pending => return Poll::Pending,
                }
            }
        }

        let begin = std::time::Instant::now();
        match this.future.poll(cx) {
            Poll::Ready(r) => Poll::Ready(r),
            Poll::Pending => {
                let elapsed = begin.elapsed().as_micros();

                // println!("elapsed: {}", elapsed);
                *this.adding_up_time += elapsed as u64;
                pendcnt.fetch_add(1, Ordering::Release);
                Poll::Pending
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::priority::Priority;
//     // use crate::test_effect;
//     use std::sync::{Arc, Mutex};

//     // #[tokio::test]
//     // async fn test_pend_cnt() {
//     //     test_pend_cnt!(time_mode);
//     // }

//     #[tokio::test]
//     async fn test_effect() {
//         test_effect!(mode_time);
//     }
// }
