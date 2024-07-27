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
    pub fn fixed_period_mode_period(&self) -> Option<u32> {
        match self {
            Priority::VeryLow => Some(2),
            Priority::Low => Some(3),
            Priority::Middle => Some(4),
            Priority::High => Some(11),
            Priority::VeryHigh => None,
        }
    }
}
