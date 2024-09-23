use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Sender,
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ratelimit::Ratelimiter;

use crate::priority::{self, Priority};

#[derive(Clone)]
pub(crate) struct RateLimiterRuntimeHandle(
    pub(crate) Option<Arc<Ratelimiter>>,
    // stop flag, pull time uploader, recent_total_pull_time
    pub(crate) Arc<(AtomicBool, Sender<(Ms10, MicroSec)>, AtomicU64, AtomicU64)>,
    pub(crate) Priority,
);

#[derive(Clone)]
pub(crate) struct CommonRuntimeHandle(pub(crate) Priority);

impl CommonRuntimeHandle {
    pub(crate) fn new(priority: Priority) -> Self {
        Self(priority)
    }
}

impl Drop for RateLimiterRuntimeHandle {
    fn drop(&mut self) {
        self.1 .0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(crate) type Ms10 = u64;
pub(crate) type MicroSec = u64;

impl RateLimiterRuntimeHandle {
    pub(crate) fn new(priority: Priority) -> Self {
        // 传入时间戳以及对应的执行时间
        let (new_collect_tx, new_collect_rx) = std::sync::mpsc::channel::<(Ms10, MicroSec)>();
        let handle_dropped = Arc::new((
            AtomicBool::new(false),
            new_collect_tx,
            AtomicU64::new(0),
            AtomicU64::new(0),
        ));
        let handle_dropped2 = handle_dropped.clone();

        // std::thread::spawn(move || {
        //     // let mut timepoint_2_poll_sum_time = BTreeMap::new();
        //     while !handle_dropped2.0.load(std::sync::atomic::Ordering::Relaxed) {
        //         let v = handle_dropped2.2.swap(0, Ordering::SeqCst);
        //         println!(
        //             "reset recent time sum {} {}, .2 addr:{}",
        //             v,
        //             handle_dropped2.2.load(Ordering::SeqCst),
        //             (&handle_dropped2.2) as *const _ as usize,
        //         );
        //         thread::sleep(Duration::from_millis(10));
        //         // let now_ms10 = SystemTime::now()
        //         //     .duration_since(UNIX_EPOCH)
        //         //     .unwrap()
        //         //     .as_millis() as Ms10
        //         //     / 10;
        //         // // compute new adjust value
        //         // if let Ok(v) = new_collect_rx.recv_timeout(Duration::from_millis(10)) {
        //         //     // add to
        //         //     if v.0 >= now_ms10 - 10 {
        //         //         timepoint_2_poll_sum_time
        //         //             .entry(v.0)
        //         //             .and_modify(|pollsum| {
        //         //                 *pollsum += v.1;
        //         //             })
        //         //             .or_insert(v.1);
        //         //     }

        //         //     while let Ok(v) = new_collect_rx.try_recv() {
        //         //         // add to
        //         //         if v.0 >= now_ms10 - 10 {
        //         //             timepoint_2_poll_sum_time
        //         //                 .entry(v.0)
        //         //                 .and_modify(|pollsum| {
        //         //                     *pollsum += v.1;
        //         //                 })
        //         //                 .or_insert(v.1);
        //         //         }
        //         //     }
        //         // }
        //         // // timepoint_2_poll_sum_time.retain(|v|{})
        //         // let mut sum = 0;
        //         // timepoint_2_poll_sum_time.retain(|timepoint, pull_sum_time| {
        //         //     if *timepoint >= now_ms10 - 10 {
        //         //         sum += *pull_sum_time;
        //         //         true
        //         //     } else {
        //         //         false
        //         //     }
        //         // });
        //         // handle_dropped2.2.store(sum, Ordering::Relaxed);
        //         // println!("update recent time sum {}", sum);
        //     }
        //     println!("system stopped");
        // });
        match priority {
            Priority::VeryLow => Self(
                Some(Arc::new(
                    Ratelimiter::builder(800, Duration::from_millis(70)) // 10ms 生产100个，即10ms内只允许100次poll
                        .max_tokens(2000) // 预留资源，避免突发
                        .build()
                        .unwrap(),
                )),
                handle_dropped,
                priority,
            ),
            Priority::Low => Self(
                Some(Arc::new(
                    Ratelimiter::builder(1100, Duration::from_millis(50)) // 10ms 生产100个，即10ms内只允许100次poll
                        .max_tokens(2000) // 预留资源，避免突发
                        .build()
                        .unwrap(),
                )),
                handle_dropped,
                priority,
            ),
            Priority::Middle => Self(
                Some(Arc::new(
                    Ratelimiter::builder(1400, Duration::from_millis(30)) // 10ms 生产100个，即10ms内只允许100次poll
                        .max_tokens(2000) // 预留资源，避免突发
                        .build()
                        .unwrap(),
                )),
                handle_dropped,
                priority,
            ),
            Priority::High => Self(
                Some(Arc::new(
                    Ratelimiter::builder(1700, Duration::from_millis(10)) // 10ms 生产100个，即10ms内只允许100次poll
                        .max_tokens(2000) // 预留资源，避免突发
                        .build()
                        .unwrap(),
                )),
                handle_dropped,
                priority,
            ),
            Priority::VeryHigh => Self(None, handle_dropped, priority),
        }
    }
}
