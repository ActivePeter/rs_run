use std::{
    cell::RefCell,
    future::Future,
    ptr::NonNull,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    task::{Context, Poll, Waker},
};

// 定义一个 Future 包装器
struct PendingFuture<F: Future + Send + 'static> {
    future: F,
    result: Option<F::Output>,
    // wakers: Vec<Waker>,
    #[cfg(test)]
    pend_cnt: Arc<usize>,
}

impl<F> Future for PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    type Output = F::Output;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        println!("polling");
        // if let Some(result) = self.as_mut().get_mut().result.take() {
        //     return Poll::Ready(result);
        // }
        let mut result: NonNull<Option<<F as Future>::Output>> =
            NonNull::new(&self.result as *const _ as *mut _).unwrap();

        #[cfg(test)]
        let mut pend_cnt = NonNull::new(&*self.pend_cnt as *const usize as *mut usize).unwrap();

        if let Some(result) = unsafe { result.as_mut().take() } {
            println!("pended and ready");
            return Poll::Ready(result);
        }
        let inner_future = unsafe { self.map_unchecked_mut(|v| &mut v.future) };

        match inner_future.poll(cx) {
            Poll::Ready(r) => {
                println!("ready but pend");
                unsafe {
                    *result.as_mut() = Some(r);
                }
                // for waker in self.wakers.drain(..) {
                //     waker.wake();
                // }
                #[cfg(test)]
                unsafe {
                    *pend_cnt.as_mut() += 1;
                }
                cx.waker().clone().wake();
                Poll::Pending
                // Poll::Ready(r)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::PendingFuture;

    #[tokio::test]
    async fn test_pend_logic() {
        let pend_cnt = Arc::new(0);
        let future = PendingFuture {
            future: async { 42 },
            result: None,
            pend_cnt: pend_cnt.clone(),
        };
        tokio::spawn(async move {
            let _ = future.await;
        })
        .await
        .unwrap();
        assert!(*pend_cnt == 1);
    }

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
    async fn test_multi_await_no_io() {
        let pend_cnt = Arc::new(0);
        let future = PendingFuture {
            future: async {
                // on stack future
                hello1().await;
                hello2().await;
                // box dyn future
                let f = Hello.hello();
                f.await;
                async { 42 }.await;
                42
            },
            result: None,
            pend_cnt: pend_cnt.clone(),
        };
        tokio::spawn(async move {
            let _ = future.await;
        })
        .await
        .unwrap();
        assert!(*pend_cnt == 1);
    }
}
