use futures::FutureExt;
use parking_lot::lock_api::RwLock;
use parking_lot::Mutex;
use prometheus::core::{Atomic, AtomicF64};
use rand::{thread_rng, Rng};
use ratelimit::Ratelimiter;
use tokio::time::Sleep;

// use core::panicking::panic;
use std::any::TypeId;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::task::Waker;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll},
};

use super::runtime_handle::{CommonRuntimeHandle, Ms10, RateLimiterRuntimeHandle};
use crate::priority::{self, Priority};

struct OneFutureRecord {
    total_micro: u64,
    cpu_micro: u64,
}

// static ref EACH_FUTURE_RECORD: parking_lot::RwLock<Option<std::sync::mpsc::Sender<(Priority,Waker)>>> = RwLock::new(None);

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
    handle: RateLimiterRuntimeHandle,
    /// count of pendings
    pub pend_cnt: u32, // track the pending count for test
    /// count of inserted pendings
    // pub inserted_pend_cnt: u32,
    // sche_time: u32,
    // poll_time: u32,
    state: State,
}

impl<F> PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    pub fn new(handle: RateLimiterRuntimeHandle, future: F) -> Self {
        Self {
            future,
            handle,
            pend_cnt: 0,
            // inserted_pend_cnt: 0,
            state: State::Common,
            // poll_time: 0,
            // sche_time: 0,
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
        // let sche_begin = Instant::now();
        match this.state {
            State::Common => {}
            State::Backoff(ref mut sleep) => match sleep.poll_unpin(cx) {
                Poll::Ready(_) => {
                    *this.state = State::Common;
                    this.handle.1 .3.fetch_sub(1, Ordering::Relaxed);
                }
                Poll::Pending => return Poll::Pending,
            },
        };

        // let inter = 5;

        // if (*this.pend_cnt + 1) % inter == 0 {
        if let Some(ratelimiter) = &this.handle.0 {
            *this.pend_cnt += 1;
            if let Err(wait) = ratelimiter.try_wait() {
                *this.state = State::Backoff(Box::pin(tokio::time::sleep(wait)));
                match this.state.unwrap_backoff().poll_unpin(cx) {
                    Poll::Ready(_) => {
                        *this.state = State::Common;
                        cx.waker().clone().wake();
                        return Poll::Pending;
                    }
                    Poll::Pending => {
                        return Poll::Pending;
                    }
                }
            }
        }
        // }
        let poll_res = this.future.poll(cx);

        match poll_res {
            Poll::Ready(r) => Poll::Ready(r),
            Poll::Pending => {
                *this.pend_cnt += 1;
                Poll::Pending
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::PendingFuture;
//     use crate::priority::Priority;
//     use crate::{test_effect, test_pend_cnt};
//     use std::sync::atomic::Ordering;

//     async fn hello1() {
//         println!("hello1");
//     }

//     async fn hello2() {
//         println!("hello2");
//     }

//     #[async_trait::async_trait]
//     trait AsyncTraitHello {
//         async fn hello(&self);
//     }

//     struct Hello;
//     #[async_trait::async_trait]
//     impl AsyncTraitHello for Hello {
//         async fn hello(&self) {
//             println!("hello3")
//         }
//     }

//     #[tokio::test]
//     async fn test_multi_await_no_pend() {
//         let future = PendingFuture::new(Priority::Middle, async {
//             // on stack future
//             hello1().await;
//             hello2().await;
//             // box dyn future,
//             // The absence of pend could be due to compiler optimizations,
//             //  or it could be related to the future generation mechanism.
//             let f = Hello.hello();
//             f.await;
//             async { 42 }.await;
//             42
//         });
//         let pend_cnt = future.pend_cnt.clone();

//         tokio::spawn(async move {
//             let _ = future.await;
//         })
//         .await
//         .unwrap();
//         assert_eq!(0, pend_cnt.load(Ordering::Acquire));
//     }

//     #[tokio::test]
//     async fn test_pend_cnt() {
//         test_pend_cnt!(mode_count);
//     }

//     #[tokio::test]
//     async fn test_effect() {
//         test_effect!(mode_count);
//     }
// }
