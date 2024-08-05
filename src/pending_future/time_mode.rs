use std::{
    cell::RefCell,
    future::Future,
    ptr::NonNull,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    task::{Context, Poll, Waker},
};

use crate::priority::Priority;

pub fn new_pending_future<F>(priority: Priority, future: F) -> TimeModePendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    TimeModePendingFuture::new(priority, future)
}

pub struct TimeModePendingFuture<F: Future + Send + 'static> {
    future: F,
    /// priority of this future
    priority: Priority,
    /// count of inner future pending
    pub pend_cnt: Arc<u32>, // track the pending count for test
    pub inserted_pend_cnt: Arc<u32>,
    adding_up_time: Arc<u64>,
}

impl<F> TimeModePendingFuture<F>
where
    F: Future + Send + 'static,
{
    fn new(priority: Priority, future: F) -> Self {
        Self {
            future,
            pend_cnt: Arc::new(0),
            priority,
            adding_up_time: Arc::new(0),
            inserted_pend_cnt: Arc::new(0),
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
        let pend_period = self.priority.fixed_interval_time_ms();

        let mut pendcnt = NonNull::new(&*self.pend_cnt as *const _ as *mut u32).unwrap();
        let mut inserted_pendcnt =
            NonNull::new(&*self.inserted_pend_cnt as *const _ as *mut u32).unwrap();
        let mut adding_up_time =
            NonNull::new(&*self.adding_up_time as *const _ as *mut u64).unwrap();

        // println!("adding_up_time: {}", self.adding_up_time);

        if let Some(pend_period) = pend_period {
            unsafe {
                if *adding_up_time.as_ref() > pend_period {
                    *adding_up_time.as_mut() -= pend_period;
                    *inserted_pendcnt.as_mut() += 1;
                    // Register the current task to be woken when the pending period is reached.
                    let waker = cx.waker().clone();
                    waker.wake();

                    return Poll::Pending;
                }
            }
        }

        let inner_future = unsafe { self.map_unchecked_mut(|v| &mut v.future) };

        let begin = std::time::Instant::now();
        match inner_future.poll(cx) {
            Poll::Ready(r) => Poll::Ready(r),
            Poll::Pending => {
                let elapsed = begin.elapsed().as_micros();

                // println!("elapsed: {}", elapsed);
                unsafe {
                    *adding_up_time.as_mut() += elapsed as u64;
                    *pendcnt.as_mut() += 1;
                }
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::priority::Priority;
    use crate::test_effect;
    use crate::test_pend_cnt;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    // #[tokio::test]
    // async fn test_pend_cnt() {
    //     test_pend_cnt!(time_mode);
    // }

    #[tokio::test]
    async fn test_effect() {
        test_effect!(time_mode);
    }
}
