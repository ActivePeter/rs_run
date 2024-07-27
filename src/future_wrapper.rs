use std::{
    cell::RefCell,
    future::Future,
    ptr::NonNull,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    task::{Context, Poll, Waker},
};

use crate::priority::Priority;

// 定义一个 Future 包装器
pub(crate) struct PendingFuture<F: Future + Send + 'static> {
    future: F,
    /// priority of this future
    priority: Priority,

    /// count of inner future pending
    pend_cnt: Arc<u32>, // track the pending count for test
}

impl<F> PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    pub(crate) fn new(priority: Priority, future: F) -> Self {
        Self {
            future,
            pend_cnt: Arc::new(0),
            priority,
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
        let pend_period = self.priority.fixed_period_mode_period();

        let mut pendcnt = unsafe { NonNull::new(&*self.pend_cnt as *const _ as *mut u32).unwrap() };
        if let Some(pend_period) = pend_period {
            unsafe {
                if (*pendcnt.as_ref() + 1) % pend_period == 0 {
                    // println!("pending");
                    *pendcnt.as_mut() += 1;

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

    use crate::priority::Priority;

    use super::PendingFuture;

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
    async fn test_fixed_period_priority_mode() {
        async fn test_spec_priority(priority: Priority) {
            let period = priority.fixed_period_mode_period();

            // no trigger pend case
            let (pend_cnt, should_pend) = if let Some(period) = period {
                let f = PendingFuture::new(priority, async move {
                    for _ in 0..period - 2 {
                        tokio::task::yield_now().await;
                    }
                });
                let pend_cnt = f.pend_cnt.clone();
                f.await;
                (pend_cnt, period - 2)
            } else {
                let f = PendingFuture::new(priority, async {
                    for _ in 0..100 {
                        tokio::task::yield_now().await;
                    }
                });
                let pend_cnt = f.pend_cnt.clone();
                f.await;
                (pend_cnt, 100)
            };

            assert!(
                *pend_cnt == should_pend,
                "{} {} {:?}",
                *pend_cnt,
                should_pend,
                priority
            );

            // no trigger pend case
            let Some(period) = period else {
                return;
            };

            // let period_move = period;
            let (inner_future, should_pend) = {
                (
                    async move {
                        for _ in 0..period - 1 {
                            tokio::task::yield_now().await;
                        }
                    },
                    period,
                )
            };

            let future = PendingFuture::new(priority, inner_future);
            let pend_cnt = future.pend_cnt.clone();
            future.await;
            assert!(*pend_cnt == should_pend);
        }
        test_spec_priority(Priority::VeryLow).await;
        test_spec_priority(Priority::Low).await;
        test_spec_priority(Priority::Middle).await;
        test_spec_priority(Priority::High).await;
        test_spec_priority(Priority::VeryHigh).await;
    }

    #[tokio::test]
    async fn test_fixed_period_priority_mode_effect() {
        async fn compute_heavily() {
            let mut result = 0u64;

            // Simulate a heavy computation loop.
            for _ in 0..1_000_000 {
                result += 1;
                if result % 1_000 == 0 {
                    // Yield the current task to give other tasks a chance to run.
                    tokio::task::yield_now().await;
                }
            }

            println!("Computation completed: {}", result);
        }
        println!("start test_fixed_period_priority_mode_effect");
        let mut tasks = vec![];
        let mut each_task_time_collect: HashMap<Priority, Mutex<Vec<usize>>> = HashMap::new();
        for priority in &[
            Priority::VeryLow,
            Priority::Low,
            Priority::Middle,
            Priority::High,
            Priority::VeryHigh,
        ] {
            each_task_time_collect.insert(*priority, Mutex::new(vec![]));
        }
        let each_task_time_collect = Arc::new(each_task_time_collect);
        for i in 0..100 {
            for priority in &[
                Priority::VeryLow,
                Priority::Low,
                Priority::Middle,
                Priority::High,
                Priority::VeryHigh,
            ] {
                let each_task_time_collect = each_task_time_collect.clone();
                println!("spawn task {:?} {}", priority, i);
                let task = tokio::spawn(async move {
                    println!("start task {:?} {}", priority, i);
                    let begin = std::time::Instant::now();
                    let f = PendingFuture::new(*priority, compute_heavily());
                    f.await;
                    let elapsed = begin.elapsed().as_millis() as usize;
                    each_task_time_collect
                        .get(priority)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push(elapsed);
                });
                tasks.push(task);
            }
        }
        for task in tasks {
            task.await.unwrap();
        }
        for priority in &[
            Priority::VeryLow,
            Priority::Low,
            Priority::Middle,
            Priority::High,
            Priority::VeryHigh,
        ] {
            let time_collect = each_task_time_collect.get(priority).unwrap();
            let time_collect = time_collect.lock().unwrap();
            let mut sum = 0;
            for time in time_collect.iter() {
                sum += time;
            }
            println!(
                "priority {:?} average time: {}",
                priority,
                sum as f64 / time_collect.len() as f64
            );
        }
    }
}
