use std::{
    cell::RefCell,
    future::Future,
    ptr::NonNull,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    task::{Context, Poll, Waker},
};

use crate::priority::{self, Priority};

pub fn new_pending_future<F>(priority: Priority, future: F) -> PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    PendingFuture::new(priority, future)
}

pub struct PendingFuture<F: Future + Send + 'static> {
    future: F,
    /// priority of this future
    priority: Priority,
    /// count of pend
    pub pend_cnt: Arc<u32>, // track the pending count for test
    /// count of inserted pend
    pub inserted_pend_cnt: Arc<u32>,
}

impl<F> PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    pub fn new(priority: Priority, future: F) -> Self {
        Self {
            future,
            pend_cnt: Arc::new(0),
            priority,
            inserted_pend_cnt: Arc::new(0),
        }
    }
}

// impl<F> PendingFuture<F> for CountModePendingFuture<F>
// where
//     F: Future + Send + 'static,
//     F::Output: Send + 'static,
// {
//     fn new(priority: Priority, future: F) -> Self {
//         Self::new(priority, future)
//     }

//     fn pend_cnt_track(&self) -> Arc<u32> {
//         self.pend_cnt.clone()
//     }
// }

impl<F> Future for PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    type Output = F::Output;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pend_period = self.priority.fixed_interval_count();

        let mut pendcnt = NonNull::new(&*self.pend_cnt as *const _ as *mut u32).unwrap();
        let mut inserted_pendcnt =
            NonNull::new(&*self.inserted_pend_cnt as *const _ as *mut u32).unwrap();

        if let Some(pend_period) = pend_period {
            unsafe {
                if (*pendcnt.as_ref() + 1) % pend_period == 0 {
                    // println!("pending");
                    *pendcnt.as_mut() += 1;
                    *inserted_pendcnt.as_mut() += 1;

                    // Register the current task to be woken when the pending period is reached.
                    let waker = cx.waker().clone();
                    waker.wake();

                    return Poll::Pending;
                }
            }
        }

        let inner_future = unsafe { self.map_unchecked_mut(|v| &mut v.future) };

        match inner_future.poll(cx) {
            Poll::Ready(r) => Poll::Ready(r),
            Poll::Pending => {
                unsafe {
                    // println!("pending");
                    *pendcnt.as_mut() += 1;
                }
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::{
        collections::HashMap,
        future,
        sync::{Arc, Mutex},
    };

    use super::PendingFuture;
    use crate::priority::Priority;
    use crate::test_effect;
    use crate::test_pend_cnt;

    async fn hello1() {
        println!("hello1");
    }

    async fn hello2() {
        println!("hello2");
    }

    #[async_trait::async_trait]
    trait AsyncTraitHello {
        async fn hello(&self);
    }

    struct Hello;
    #[async_trait::async_trait]
    impl AsyncTraitHello for Hello {
        async fn hello(&self) {
            println!("hello3")
        }
    }

    #[tokio::test]
    async fn test_multi_await_no_pend() {
        let future = PendingFuture::new(crate::priority::Priority::Middle, async {
            // on stack future
            hello1().await;
            hello2().await;
            // box dyn future,
            // The absence of pend could be due to compiler optimizations,
            //  or it could be related to the future generation mechanism.
            let f = Hello.hello();
            f.await;
            async { 42 }.await;
            42
        });
        let pend_cnt = future.pend_cnt.clone();

        tokio::spawn(async move {
            let _ = future.await;
        })
        .await
        .unwrap();
        assert!(*pend_cnt == 0);
    }

    #[tokio::test]
    async fn test_pend_cnt() {
        test_pend_cnt!(count_mode);
    }

    #[tokio::test]
    async fn test_effect() {
        test_effect!(count_mode);
    }
}
