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
use std::time::{Duration, Instant};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll},
};

use super::runtime_handle::CommonRuntimeHandle;
use crate::priority::{self, Priority};

struct OneFutureRecord {
    total_micro: u64,
    cpu_micro: u64,
}

// static ref EACH_FUTURE_RECORD: parking_lot::RwLock<Option<std::sync::mpsc::Sender<(Priority,Waker)>>> = RwLock::new(None);

// lazy_static::lazy_static! {
//     static ref EACH_FUTURE_RECORD: parking_lot::RwLock<HashMap<TypeId, VecDeque<OneFutureRecord>>>> = RwLock::new(HashMap::new());
// }

lazy_static::lazy_static! {
    static ref EACH_FUTURE_RECORD: parking_lot::RwLock<HashMap<TypeId, (Arc<AtomicF64>,Arc<Mutex<VecDeque<OneFutureRecord>>>)>> = RwLock::new(HashMap::new());
}

// fn delay_future(priority: Priority, waker: Waker) {
//     if let Some(a) = EACH_FUTURE_RECORD.read().as_ref() {
//         a.send((priority, waker)).unwrap();
//     } else {
//         let mut write = EACH_FUTURE_RECORD.write();
//         if let Some(a) = &*write {
//             a.send((priority, waker)).unwrap();
//         } else {
//             let (tx, rx) = std::sync::mpsc::channel();
//             *write = Some(tx);
//             std::thread::spawn(move || {
//                 let mut loop_cnt = 0;
//                 let mut timeout_wakers: BTreeMap<i32, Vec<Waker>> = BTreeMap::new();
//                 loop {
//                     // let laft_time_2_wakers=HashMap<>
//                     let new_delay = rx.recv_timeout(Duration::from_millis(1));
//                     if let Some(delays) = timeout_wakers.remove(&loop_cnt) {
//                         for waker in delays {
//                             waker.wake();
//                         }
//                     }
//                     if let Ok((priority, waker)) = new_delay {
//                         // let new_delays = vec![new_dalay];
//                         let delay = match priority {
//                             Priority::VeryLow => 4,
//                             Priority::Low => 3,
//                             Priority::Middle => 2,
//                             Priority::High => 1,
//                             Priority::VeryHigh => panic!("can't delay for very high"),
//                         };
//                         let mut waker_opt = Some(waker);
//                         timeout_wakers
//                             .entry(loop_cnt + delay)
//                             .and_modify(|v| v.push(waker_opt.take().unwrap()))
//                             .or_insert(vec![waker_opt.take().unwrap()]);
//                         while let Ok((priority, waker)) = rx.try_recv() {
//                             let delay = match priority {
//                                 Priority::VeryLow => 4,
//                                 Priority::Low => 3,
//                                 Priority::Middle => 2,
//                                 Priority::High => 1,
//                                 Priority::VeryHigh => panic!("can't delay for very high"),
//                             };
//                             let mut waker_opt = Some(waker);
//                             timeout_wakers
//                                 .entry(loop_cnt + delay)
//                                 .and_modify(|v| v.push(waker_opt.take().unwrap()))
//                                 .or_insert(vec![waker_opt.take().unwrap()]);
//                         }
//                     }
//                     loop_cnt += 1;
//                 }
//             });
//         }
//     }
// }

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
    /// count of pendings
    pub pend_cnt: u32, // track the pending count for test
    /// count of inserted pendings
    // pub inserted_pend_cnt: u32,
    state: State,
    begin_time: Option<Instant>,
    // last_pend_time: Option<Instant>,
    poll_time: u64,
}

impl<F> PendingFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    pub fn new(handle: CommonRuntimeHandle, future: F) -> Self {
        Self {
            future,
            handle,
            pend_cnt: 0,
            // inserted_pend_cnt: 0,
            state: State::Common,
            begin_time: None,
            // last_pend_time: None,
            poll_time: 0,
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
        let this = self.project();
        let bf_poll = Instant::now();

        if this.begin_time.is_none() {
            *this.begin_time = Some(Instant::now());
            // *this.last_pend_time = *this.begin_time;
        }

        match this.state {
            State::Common => {}
            State::Backoff(ref mut sleep) => match sleep.poll_unpin(cx) {
                Poll::Ready(_) => {
                    *this.state = State::Common;
                }
                Poll::Pending => return Poll::Pending,
            },
        };

        let pend_period = this.handle.0.fixed_interval_count();
        let inter = 5;
        // match this.handle.0 {
        //     Priority::VeryLow => 5,
        //     Priority::Low => 10,
        //     Priority::Middle => 13,
        //     Priority::High => 16,
        //     Priority::VeryHigh => 10000000,
        // };

        if (*this.pend_cnt + 1) % inter == 0 {
            // if this.last_pend_time.unwrap().elapsed().as_millis() > 500 {
            // *this.last_pend_time = Some(Instant::now());

            if let Some(mut pend_period) = pend_period {
                //     let elapsed = this.begin_time.unwrap().elapsed().as_micros();
                //     if *this.poll_time > 0 && (cur_pend_cnt + 1) % inter == 0 {
                //         let cpu_rate = (*this.poll_time as u64 * 1000 / elapsed as u64);
                //         let hope_cpu_rate = pend_period as u64 * 1000 / 45;
                //         if cpu_rate > hope_cpu_rate {
                //             let diff = cpu_rate - hope_cpu_rate;
                // if (cur_pend_cnt + 1) % inter == 0 {
                // let f_recent_cpu_rate = EACH_FUTURE_RECORD
                //     .read()
                //     .get(&TypeId::of::<F>())
                //     .map(|v| v.0.get());
                // if f_recent_cpu_rate.is_some() && f_recent_cpu_rate.unwrap() >= 0.0 {
                //     let off = 5;
                //     let adj = 8.0;
                //     let old = pend_period;
                //     pend_period =
                //         off + ((pend_period as f64) * adj * f_recent_cpu_rate.unwrap()) as u32;
                //     println!(
                //         "sleep for off({}) + pend({}) * adj({}) * rate({}) = {}",
                //         off,
                //         old,
                //         adj,
                //         f_recent_cpu_rate.unwrap(),
                //         pend_period
                //     )
                // }
                // let sleep = ((diff + 9) / 7) as u64 + 5;
                // if sleep > 5 {
                //     // println!("pending");
                *this.pend_cnt += 1;
                // *this.inserted_pend_cnt += 1;
                //     // Register the current task to be woken when the pending period is reached.

                //     println!("{}-{}, will sleep", cpu_rate, hope_cpu_rate,);

                // delay_future(this.handle.0, cx.waker().clone());
                // return Poll::Pending;

                *this.state = State::Backoff(Box::pin(tokio::time::sleep(
                    std::time::Duration::from_millis(
                        pend_period as u64 + thread_rng().gen_range(0..10), // 20,
                                                                            // ((diff + 9) / 7) as u64 + 5,
                    ),
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

        let poll_res = this.future.poll(cx);
        *this.poll_time += bf_poll.elapsed().as_micros() as u64;
        match poll_res {
            Poll::Ready(r) => {
                let record = OneFutureRecord {
                    total_micro: this.begin_time.unwrap().elapsed().as_micros() as u64,
                    cpu_micro: *this.poll_time,
                };
                let each_future_record = EACH_FUTURE_RECORD.read();
                if let Some(records_) = each_future_record.get(&TypeId::of::<F>()).cloned() {
                    let mut records = records_.1.lock();
                    records.push_back(record);
                    if records.len() > 10 {
                        records.pop_front();
                    }
                    records_.0.swap(
                        records
                            .iter()
                            .map(|v| v.cpu_micro as f64 / v.total_micro as f64)
                            .sum::<f64>()
                            / records.len() as f64,
                        Ordering::Relaxed,
                    );
                } else {
                    drop(each_future_record);
                    EACH_FUTURE_RECORD.write().insert(
                        TypeId::of::<F>(),
                        (
                            Arc::new(AtomicF64::new(
                                record.cpu_micro as f64 / record.total_micro as f64,
                            )),
                            Arc::new(Mutex::new({
                                let mut v = VecDeque::new();
                                v.push_back(record);
                                v
                            })),
                        ),
                    );
                }
                Poll::Ready(r)
            }
            Poll::Pending => {
                // println!("pending");
                *this.pend_cnt += 1;
                // pendcnt.fetch_add(1, Ordering::Release);
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
