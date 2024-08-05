use std::future::Future;

use tokio::task::JoinHandle;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Priority {
    VeryLow = 0,
    Low = 1,
    Middle = 2,
    High = 3,
    VeryHigh = 4,
}

impl Priority {
    pub fn fixed_interval_count(&self) -> Option<u32> {
        match self {
            Priority::VeryLow => Some(2),
            Priority::Low => Some(3),
            Priority::Middle => Some(4),
            Priority::High => Some(11),
            Priority::VeryHigh => None,
        }
    }

    pub fn fixed_interval_time_ms(&self) -> Option<u64> {
        match self {
            Priority::VeryLow => Some(5),
            Priority::Low => Some(10),
            Priority::Middle => Some(50),
            Priority::High => Some(100),
            Priority::VeryHigh => None,
        }
    }
}
