use std::sync::atomic::{AtomicU32, Ordering};
use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind};

use crate::priority::{self, Priority};

// use super::PendingFutureMode;

// macro_rules! new_pending_future {
//     ($mode: ident) => {
//         crate::pending_future::$mode::new_pending_future
//     };
// }

#[macro_export]
macro_rules! test_pend_cnt_with_priority {
    ($mode: ident,$priority: ident) => {
        use std::sync::atomic::Ordering;
        let period = $priority.fixed_interval_count();

        // no trigger pend case
        let (pend_cnt, should_pend) = if let Some(period) = period {
            let f = crate::pending_future::$mode::new_pending_future($priority, async move {
                for _ in 0..period - 2 {
                    tokio::task::yield_now().await;
                }
            });
            let pend_cnt = f.pend_cnt.clone();
            f.await;
            (pend_cnt.load(Ordering::Acquire), period - 2)
        } else {
            let f = crate::pending_future::$mode::new_pending_future($priority, async {
                for _ in 0..100 {
                    tokio::task::yield_now().await;
                }
            });
            let pend_cnt = f.pend_cnt.clone();
            f.await;
            (pend_cnt.load(Ordering::Acquire), 100)
        };

        assert!(
            pend_cnt == should_pend,
            "{} {} {:?}",
            pend_cnt,
            should_pend,
            $priority
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

        let future = crate::pending_future::$mode::new_pending_future($priority, inner_future);
        let pend_cnt = future.pend_cnt.clone();
        future.await;
        assert!(pend_cnt.load(std::sync::atomic::Ordering::Acquire) == should_pend);
    };
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
        async fn workload_compute_heavily2() {
            let mut result = 0u64;

            // Simulate a heavy computation loop.
            for _ in 0..10_000_000 {
                result += 1;
                if result % 100_000 == 0 {
                    // Yield the current task to give other tasks a chance to run.
                    tokio::task::yield_now().await;
                }
            }

            println!("Computation completed: {}", result);
        }
        async fn workload_write_file(idx: u64) {
            use tokio::io::AsyncWriteExt;
            // make sure the directory exists
            let _ = tokio::fs::create_dir_all(TEST_GENRATE_DIR).await;

            let file_name = format!("{}/write_file{}", TEST_GENRATE_DIR, idx);
            let file = tokio::fs::File::create(file_name).await.unwrap();
            let mut file = tokio::io::BufWriter::new(file);
            for i in 0..1000 {
                file.write_all(format!("test_write_file{}\n", i).as_bytes())
                    .await
                    .unwrap();
            }
            file.flush().await.unwrap();
        }
        async fn workload_spawn_blocking_compute() {
            for _ in 0..1000 {
                tokio::task::spawn_blocking(move || {
                    let mut result = 0u64;
                    for _ in 0..1_000_00 {
                        result += 1;
                    }
                    println!("Computation completed: {}", result);
                })
                .await
                .unwrap();
            }
        }
        async fn workload_spawn_blocking_write_file() {
            use std::io::Write;
            let _ = tokio::fs::create_dir_all(TEST_GENRATE_DIR).await;
            for i in 0..1000 {
                tokio::task::spawn_blocking(move || {
                    let file_name = format!("{}/spawn_blocking_write_file{}", TEST_GENRATE_DIR, i);
                    let file = std::fs::File::create(file_name).unwrap();
                    let mut file = std::io::BufWriter::new(file);
                    // for i in 0..10 {
                    file.write_all(format!("test_spawn_blocking_write_file{}\n", i).as_bytes())
                        .unwrap();
                    // }
                    file.flush().unwrap();
                })
                .await
                .unwrap();
            }
        }
    };
}

#[macro_export]
macro_rules! test_effect {
    ($mode: ident) => {
        use std::sync::atomic::AtomicU32;
        use std::sync::Arc;
        use std::sync::Mutex;
        crate::def_workloads!();

        type WorkLoadId = u64;
        const WORKLOAD_COUNT: u64 = 5;

        println!("start test_effect");
        let mut tasks = vec![];

        // (Priority, WorkLoadId) -> Vec<(Time, InsertedPendCount)>
        let mut each_task_time_collect: std::collections::HashMap<
            (Priority, WorkLoadId),
            Mutex<Vec<(usize, Arc<AtomicU32>)>>,
        > = std::collections::HashMap::new();
        for priority in &[
            Priority::VeryLow,
            Priority::Low,
            Priority::Middle,
            Priority::High,
            Priority::VeryHigh,
        ] {
            for i in 0..WORKLOAD_COUNT {
                each_task_time_collect.insert((*priority, i), Mutex::new(vec![]));
            }
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
                    let workload_id = i % WORKLOAD_COUNT;
                    let pend_cnt = match workload_id {
                        0 => {
                            let f = crate::pending_future::$mode::new_pending_future(
                                *priority,
                                workload_compute_heavily(),
                            );
                            let cnt = f.inserted_pend_cnt.clone();
                            f.await;
                            cnt
                        }
                        1 => {
                            let f = crate::pending_future::$mode::new_pending_future(
                                *priority,
                                workload_compute_heavily2(),
                            );
                            let cnt = f.inserted_pend_cnt.clone();
                            f.await;
                            cnt
                        }
                        2 => {
                            let f = crate::pending_future::$mode::new_pending_future(
                                *priority,
                                workload_write_file(i),
                            );
                            let cnt = f.inserted_pend_cnt.clone();
                            f.await;
                            cnt
                        }
                        3 => {
                            let f = crate::pending_future::$mode::new_pending_future(
                                *priority,
                                workload_spawn_blocking_compute(),
                            );
                            let cnt = f.inserted_pend_cnt.clone();
                            f.await;
                            cnt
                        }
                        4 => {
                            let f = crate::pending_future::$mode::new_pending_future(
                                *priority,
                                workload_spawn_blocking_write_file(),
                            );
                            let cnt = f.inserted_pend_cnt.clone();
                            f.await;
                            cnt
                        }
                        _ => {
                            unreachable!()
                        }
                    };

                    let elapsed = begin.elapsed().as_millis() as usize;
                    each_task_time_collect
                        .get(&(*priority, workload_id))
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push((elapsed, pend_cnt));
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
            // let time_collect = each_task_time_collect.get(priority).unwrap();
            // let time_collect = time_collect.lock().unwrap();
            let mut sum = 0;
            let mut count = 0;
            let mut each_workload_pendcnt_per_ms = vec![0.0; WORKLOAD_COUNT as usize];

            for i in 0..WORKLOAD_COUNT {
                let time_collect = each_task_time_collect.get(&(*priority, i)).unwrap();
                let time_collect = time_collect.lock().unwrap();
                let mut sum_pendcnt_per_ms = 0.0;
                for (time, pend_cnt) in time_collect.iter() {
                    sum += time;
                    count += 1;
                    sum_pendcnt_per_ms +=
                        pend_cnt.load(std::sync::atomic::Ordering::Acquire) as f64 / *time as f64;
                }
                each_workload_pendcnt_per_ms[i as usize] =
                    sum_pendcnt_per_ms / time_collect.len() as f64;
            }
            // Calculate mean
            let mean = each_workload_pendcnt_per_ms.iter().sum::<f64>()
                / each_workload_pendcnt_per_ms.len() as f64;

            // Calculate variance
            let variance = each_workload_pendcnt_per_ms
                .iter()
                .map(|&x| {
                    let diff = x - mean;
                    diff * diff
                })
                .sum::<f64>()
                / each_workload_pendcnt_per_ms.len() as f64;

            println!(
                "priority {:?} pend rate distribution {:?} with variance {}",
                priority, each_workload_pendcnt_per_ms, variance
            );
            println!(
                "priority {:?} average time: {}",
                priority,
                sum as f64 / count as f64
            );
        }
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
            RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cpu()),
        );
        let mut cpu_usage = 0.0;
        let mut metric_count = 0;
        while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
            assert!(metric.refresh_process((pid as usize).into()));
            if let Some(process) = metric.process(Pid::from(pid as usize)) {
                cpu_usage += process.cpu_usage();
                metric_count += 1;
            }
            thread::sleep(std::time::Duration::from_millis(1));
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

#[tokio::test]
async fn test_all_cpu_usage() {
    let mut collect: BTreeMap<String, f32> = BTreeMap::new();
    for i in 0..5 {
        for p in &[
            Priority::VeryLow,
            Priority::Low,
            Priority::Middle,
            Priority::High,
            Priority::VeryHigh,
        ] {
            let metric = test_spec_priority_and_workload(*p, i).await;
            collect.insert(format!("{},{}", *p as u64, i), metric.cpu_usage);
        }
    }
    // write to json
    let writer = std::fs::File::create("priority_workload_cpu_usage.json").unwrap();

    serde_json::to_writer(writer, &collect).unwrap();
}

pub async fn test_spec_priority_and_workload(
    priority: Priority,
    workload_id: u64,
) -> ProcessMetric {
    crate::def_workloads!();
    // start monitor thread
    let (stop, rx) = start_monitor_thread();
    let mut tasks = vec![];
    let start = std::time::Instant::now();
    for i in 0..100 {
        // persist cpu usage in file: {priority}.{workload_id}
        match workload_id {
            0 => {
                let f = crate::pending_future::time_mode::new_pending_future(
                    priority,
                    workload_compute_heavily(),
                );
                tasks.push(tokio::spawn(f));
            }
            1 => {
                let f = crate::pending_future::time_mode::new_pending_future(
                    priority,
                    workload_compute_heavily2(),
                );
                tasks.push(tokio::spawn(f));
            }
            2 => {
                let f = crate::pending_future::time_mode::new_pending_future(
                    priority,
                    workload_spawn_blocking_compute(),
                );
                tasks.push(tokio::spawn(f));
            }
            3 => {
                let f = crate::pending_future::time_mode::new_pending_future(
                    priority,
                    workload_spawn_blocking_write_file(),
                );
                tasks.push(tokio::spawn(f));
            }
            4 => {
                let f = crate::pending_future::time_mode::new_pending_future(
                    priority,
                    workload_write_file(i),
                );
                tasks.push(tokio::spawn(f));
            }
            _ => {
                unreachable!()
            }
        }
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

// struct MetricManager {}
