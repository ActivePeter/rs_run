use crate::priority::Priority;
use std::sync::atomic::{AtomicU32, Ordering};
use std::{
    future::Future
    ,
    sync::Arc,
    task::{Context, Poll},
};

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
    /// count of pendings
    pub pend_cnt: Arc<AtomicU32>, // track the pending count for test
    /// count of inserted pendings
    pub inserted_pend_cnt: Arc<AtomicU32>,
}

impl<F> PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    pub fn new(priority: Priority, future: F) -> Self {
        Self {
            future,
            priority,
            pend_cnt: Arc::new(AtomicU32::new(0)),
            inserted_pend_cnt: Arc::new(AtomicU32::new(0)),
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

        let pendcnt = self.pend_cnt.clone();
        let inserted_pendcnt =
            self.inserted_pend_cnt.clone();

        if let Some(pend_period) = pend_period {
            let cur_pend_cnt = pendcnt.load(Ordering::Acquire);
            if (cur_pend_cnt) % pend_period == 0 {
                // println!("pending");
                pendcnt.fetch_add(1, Ordering::Release);
                inserted_pendcnt.fetch_add(1, Ordering::Release);
                // Register the current task to be woken when the pending period is reached.
                let waker = cx.waker().clone();
                waker.wake();
                return Poll::Pending;
            }
        }

        let inner_future = unsafe { self.map_unchecked_mut(|v| &mut v.future) };

        match inner_future.poll(cx) {
            Poll::Ready(r) => Poll::Ready(r),
            Poll::Pending => {
                // println!("pending");
                pendcnt.fetch_add(1, Ordering::Release);
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PendingFuture;
    use crate::priority::Priority;
    use crate::{test_effect, test_pend_cnt};
    use std::sync::atomic::Ordering;

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
        let future = PendingFuture::new(Priority::Middle, async {
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
        assert_eq!(0, pend_cnt.load(Ordering::Acquire));
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
