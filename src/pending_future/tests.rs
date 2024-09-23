use crate::priority::{self, Priority};
use std::sync::atomic::{AtomicU32, Ordering};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind};
use tokio::io::AsyncWriteExt;

use super::runtime_handle::CommonRuntimeHandle;

// use super::PendingFutureMode;

// macro_rules! new_pending_future {
//     ($mode: ident) => {
//         crate::pending_future::$mode::new_pending_future
//     };
// }

// #[macro_export]
// macro_rules! test_pend_cnt_with_priority {
//     ($mode: ident,$priority: ident) => {
//         use std::sync::atomic::Ordering;
//         let period = $priority.fixed_interval_count();

//         // no trigger pend case
//         let (pend_cnt, should_pend) = if let Some(period) = period {
//             let f = crate::pending_future::$mode::new_pending_future($priority, async move {
//                 for _ in 0..period - 2 {
//                     tokio::task::yield_now().await;
//                 }
//             });
//             let pend_cnt = f.pend_cnt.clone();
//             f.await;
//             (pend_cnt.load(Ordering::Acquire), period - 2)
//         } else {
//             let f = crate::pending_future::$mode::new_pending_future($priority, async {
//                 for _ in 0..100 {
//                     tokio::task::yield_now().await;
//                 }
//             });
//             let pend_cnt = f.pend_cnt.clone();
//             f.await;
//             (pend_cnt.load(Ordering::Acquire), 100)
//         };

//         assert!(
//             pend_cnt == should_pend,
//             "{} {} {:?}",
//             pend_cnt,
//             should_pend,
//             $priority
//         );

//         // no trigger pend case
//         let Some(period) = period else {
//             return;
//         };

//         // let period_move = period;
//         let (inner_future, should_pend) = {
//             (
//                 async move {
//                     for _ in 0..period - 1 {
//                         tokio::task::yield_now().await;
//                     }
//                 },
//                 period,
//             )
//         };

//         let future = crate::pending_future::$mode::new_pending_future($priority, inner_future);
//         let pend_cnt = future.pend_cnt.clone();
//         future.await;
//         assert!(pend_cnt.load(std::sync::atomic::Ordering::Acquire) == should_pend);
//     };
// }

fn compute_pi_str(precision: usize) -> String {
    let mut pi = 0.0;
    let mut sign = 1.0;

    for i in 0..precision {
        pi += sign / (2 * i + 1) as f64;
        sign *= -1.0;
    }

    pi *= 4.0;

    // 将结果转换为字符串，并保留小数点后一定位数
    format!("{:.prec$}", pi, prec = precision)
}
#[test]
fn test_compute_pi_str_time() {
    let start = std::time::Instant::now();
    compute_pi_str(10);
    println!("elapsed {}", start.elapsed().as_micros());
}

#[macro_export]
macro_rules! test_pend_cnt {
    ($mode: ident) => {
        async fn test_spec_priority(priority: Priority) {
            crate::test_pend_cnt_with_priority!($mode, priority);
        }

        test_spec_priority(Priority::VeryLow).await;
        test_spec_priority(Priority::Low).await;
        test_spec_priority(Priority::Middle).await;
        test_spec_priority(Priority::High).await;
        test_spec_priority(Priority::VeryHigh).await;
    };
}

#[macro_export]
macro_rules! def_workloads {
    () => {
        const TEST_GENRATE_DIR: &'static str = "test_genrate_dir";

        async fn workload_compute_heavily() {
            let prefix = 10;
            // let mut file = tokio::fs::OpenOptions::new()
            //     .write(true)
            //     .append(true)
            //     .create(true)
            //     .open(format!("{}/pi_{}", TEST_GENRATE_DIR, prefix))
            //     .await
            //     .unwrap();
            for i in 0..3000 {
                let pi = compute_pi_str(prefix);
                tokio::task::yield_now().await;
                std::thread::yield_now();
                // println!("{} {}", i, pi);
                // file.write_all(pi.as_bytes()).await.unwrap();
            }
        }
        async fn workload_compute_heavily2() {
            let prefix = 30;
            // let mut file = tokio::fs::OpenOptions::new()
            //     .write(true)
            //     .append(true)
            //     .create(true)
            //     .open(format!("{}/pi_{}", TEST_GENRATE_DIR, prefix))
            //     .await
            //     .unwrap();
            for i in 0..2000 {
                let pi = compute_pi_str(prefix);
                tokio::task::yield_now().await;
                std::thread::yield_now();
                // println!("{} {}", i, pi);
                // file.write_all(pi.as_bytes()).await.unwrap();
            }
        }
        async fn workload_write_file(idx: u64) {
            use tokio::io::AsyncWriteExt;
            let prefix = 100;
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .create(true)
                .open(format!("{}/pi_{}", TEST_GENRATE_DIR, prefix))
                .await
                .unwrap();
            for i in 0..100 {
                let pi = compute_pi_str(prefix);

                file.write_all(pi.as_bytes()).await.unwrap();
            }
        }
        async fn workload_spawn_blocking_write_file() {
            use std::io::Write;
            let prefix = 100;
            let mut file = Some(
                std::fs::OpenOptions::new()
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(format!("{}/pi_{}", TEST_GENRATE_DIR, prefix))
                    .unwrap(),
            );
            // let mut file = tokio::fs::OpenOptions::new()
            //     .write(true)
            //     .append(true)
            //     .create(true)
            //     .open(format!("{}/pi_{}", TEST_GENRATE_DIR, prefix))
            //     .await
            //     .unwrap();
            for i in 0..100 {
                let pi = compute_pi_str(prefix);
                let mut file1 = file.take().unwrap();
                file = Some(
                    tokio::task::spawn_blocking(move || {
                        file1.write_all(pi.as_bytes()).unwrap();
                        file1
                    })
                    .await
                    .unwrap(),
                );
                // file.write_all(pi.as_bytes()).await.unwrap();
            }
        }
    };
}

// #[macro_export]
// macro_rules! def_runtime_handle_routed {
//     (common) => {
//         vec![
//             CommonRuntimeHandle(Priority::VeryLow),
//             CommonRuntimeHandle(Priority::Low),
//             CommonRuntimeHandle(Priority::Middle),
//             CommonRuntimeHandle(Priority::High),
//             CommonRuntimeHandle(Priority::VeryHigh),
//         ]
//     };
//     (ratelimit) => {

//     };
// }

#[macro_export]
macro_rules! def_runtime_handletype_route {
    (mode_count) => {
        CommonRuntimeHandle
    };
    (mode_time) => {
        CommonRuntimeHandle
    };
    (mode_ratelimiter) => {
        RateLimiterRuntimeHandle
    };
}

#[macro_export]
macro_rules! def_runtime_handles_route {
    ($mode:ident) => {
        // def_runtime_handle_routed!(common)
        vec![
            <def_runtime_handletype_route!($mode)>::new(Priority::VeryLow),
            <def_runtime_handletype_route!($mode)>::new(Priority::Low),
            <def_runtime_handletype_route!($mode)>::new(Priority::Middle),
            <def_runtime_handletype_route!($mode)>::new(Priority::High),
            <def_runtime_handletype_route!($mode)>::new(Priority::VeryHigh),
        ]
    };
}

#[derive(Debug)]
pub struct ProcessMetric {
    pub cpu_usage: f32,
}

// return (stop flag, stop receiver)
pub fn start_monitor_thread() -> (
    Arc<AtomicBool>,
    tokio::sync::oneshot::Receiver<ProcessMetric>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let pid = std::process::id();
    thread::spawn(move || {
        // monitor cpu usage
        let mut metric = sysinfo::System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );
        let mut cpu_usage = 0.0;
        let mut metric_count = 0;
        while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
            metric.refresh_cpu();
            // let process = metric.process(Pid::from(pid as usize)).unwrap();
            cpu_usage += metric.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>();
            metric_count += 1;

            // thread::sleep(std::time::Duration::from_millis(1));
            thread::yield_now();
        }
        let cpu_usage = if metric_count == 0 {
            0.0
        } else {
            cpu_usage / metric_count as f32
        };
        tx.send(ProcessMetric { cpu_usage }).unwrap();
    });

    (stop, rx)
}

// #[tokio::test]
// #[tokio::test(flavor = "multi_thread", worker_threads = 100)]
#[tokio::test]
async fn test_all_cpu_usage_mode_time() {
    use crate::def_one_mode_test;
    def_one_mode_test!(mode_time);
    test_all_cpu_usage().await;
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 16)]
#[tokio::test]
async fn test_all_cpu_usage_mode_count() {
    use crate::def_one_mode_test;
    def_one_mode_test!(mode_count);
    test_all_cpu_usage().await;
}

// #[tokio::test(flavor = "multi_thread", worker_threads = 16)]
#[tokio::test]
async fn test_all_cpu_usage_mode_ratelimiter() {
    use crate::def_one_mode_test;
    def_one_mode_test!(mode_ratelimiter);
    test_all_cpu_usage().await;
}

#[macro_export]
macro_rules! def_one_mode_test {
    ($mode:ident) => {
        use crate::pending_future::runtime_handle::*;

        async fn test_all_cpu_usage() {
            let mut collect: BTreeMap<String, f32> = BTreeMap::new();
            let runtime_handles = def_runtime_handles_route!($mode);
            let priorities = [
                Priority::VeryLow,
                Priority::Low,
                Priority::Middle,
                Priority::High,
                Priority::VeryHigh,
            ];
            for i in 0..4 {
                for (p, runtime_handle) in priorities.iter().zip(&runtime_handles) {
                    let metric = test_spec_priority_and_workload(*p, runtime_handle, i).await;
                    collect.insert(format!("{},{}", *p as u64, i), metric.cpu_usage);
                }
            }
            // write to json
            let writer = std::fs::File::create("priority_workload_cpu_usage.json").unwrap();

            serde_json::to_writer(writer, &collect).unwrap();
        }

        pub async fn test_spec_priority_and_workload(
            priority: Priority,
            runtime_handle: &def_runtime_handletype_route!($mode),
            workload_id: u64,
        ) -> ProcessMetric {
            crate::def_workloads!();
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            println!(
                "testing cpu usage for priority {:?} workload_id {}",
                priority, workload_id,
            );
            // start monitor thread
            let (stop, rx) = start_monitor_thread();
            let mut tasks = vec![];
            let start = std::time::Instant::now();
            for i in 0..500 {
                // persist cpu usage in file: {priority}.{workload_id}
                match workload_id {
                    0 => {
                        let f = crate::pending_future::$mode::PendingFuture::new(
                            runtime_handle.clone(),
                            workload_compute_heavily(),
                        );
                        tasks.push(tokio::spawn(f));
                    }
                    1 => {
                        let f = crate::pending_future::$mode::PendingFuture::new(
                            runtime_handle.clone(),
                            workload_compute_heavily2(),
                        );
                        tasks.push(tokio::spawn(f));
                    }
                    2 => {
                        let f = crate::pending_future::$mode::PendingFuture::new(
                            runtime_handle.clone(),
                            workload_spawn_blocking_write_file(),
                        );
                        tasks.push(tokio::spawn(f));
                    }
                    3 => {
                        let f = crate::pending_future::$mode::PendingFuture::new(
                            runtime_handle.clone(),
                            workload_write_file(i),
                        );
                        tasks.push(tokio::spawn(f));
                    }
                    id => {
                        panic!("invalid workload_id {}", id);
                    }
                }
                // if i % 100 == 0 {
                //     tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                // }
            }
            for task in tasks {
                task.await.unwrap();
            }
            let elapsed = start.elapsed();
            println!(
                "test cpu usage for priority {:?} workload_id {} elapsed {}ms",
                priority,
                workload_id,
                elapsed.as_millis()
            );
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
            rx.await.unwrap()
        }
    };
}

// struct MetricManager {}
