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
            // Priority::VeryLow => Some(2),
            // Priority::Low => Some(3),
            // Priority::Middle => Some(4),
            // Priority::High => Some(11),
            // Priority::VeryHigh => None,

            // Priority::VeryLow => Some(200),
            // Priority::Low => Some(150),
            // Priority::Middle => Some(100),
            // Priority::High => Some(50),
            // Priority::VeryHigh => None,
            Priority::VeryLow => Some(30),
            Priority::Low => Some(20),
            Priority::Middle => Some(13),
            Priority::High => Some(10),
            Priority::VeryHigh => None,
            // Priority::VeryLow => Some(9),
            // Priority::Low => Some(14),
            // Priority::Middle => Some(19),
            // Priority::High => Some(24),
            // Priority::VeryHigh => None,
        }
    }

    pub fn fixed_interval_time_ms(&self) -> Option<u64> {
        match self {
            Priority::VeryLow => Some(9),
            Priority::Low => Some(7),
            Priority::Middle => Some(5),
            Priority::High => Some(3),
            Priority::VeryHigh => None,
        }
    }
}
