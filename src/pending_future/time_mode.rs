use crate::priority::Priority;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll},
};

pub fn new_pending_future<F>(priority: Priority, future: F) -> TimeModePendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    TimeModePendingFuture::new(priority, future)
}

#[pin_project::pin_project]
pub struct TimeModePendingFuture<F: Future + Send + 'static> {
    #[pin]
    future: F,
    /// priority of this future
    priority: Priority,
    /// count of inner future pending
    pub pend_cnt: Arc<AtomicU32>, // track the pending count for test
    pub inserted_pend_cnt: Arc<AtomicU32>,
    adding_up_time: Arc<AtomicU64>,
}

impl<F> TimeModePendingFuture<F>
where
    F: Future + Send + 'static,
{
    fn new(priority: Priority, future: F) -> Self {
        Self {
            future,
            pend_cnt: Default::default(),
            priority,
            adding_up_time: Default::default(),
            inserted_pend_cnt: Default::default(),
        }
    }
}

impl<F> Future for TimeModePendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    type Output = F::Output;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let pend_period = this.priority.fixed_interval_time_ms();

        let pendcnt = this.pend_cnt.clone();
        let inserted_pendcnt = this.inserted_pend_cnt.clone();
        let adding_up_time = this.adding_up_time.clone();

        // println!("adding_up_time: {}", self.adding_up_time);

        if let Some(pend_period) = pend_period {
            if adding_up_time.load(Ordering::Acquire) > pend_period {
                adding_up_time.fetch_sub(pend_period, Ordering::Release);
                inserted_pendcnt.fetch_add(1, Ordering::Release);
                // Register the current task to be woken when the pending period is reached.
                let waker = cx.waker().clone();
                waker.wake();
                return Poll::Pending;
            }
        }

        let begin = std::time::Instant::now();
        match this.future.poll(cx) {
            Poll::Ready(r) => Poll::Ready(r),
            Poll::Pending => {
                let elapsed = begin.elapsed().as_micros();

                // println!("elapsed: {}", elapsed);
                adding_up_time.fetch_add(elapsed as u64, Ordering::Release);
                pendcnt.fetch_add(1, Ordering::Release);
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::priority::Priority;
    use crate::test_effect;
    use std::sync::{Arc, Mutex};

    // #[tokio::test]
    // async fn test_pend_cnt() {
    //     test_pend_cnt!(time_mode);
    // }

    #[tokio::test]
    async fn test_effect() {
        test_effect!(time_mode);
    }
}
