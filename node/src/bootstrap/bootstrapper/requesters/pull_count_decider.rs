use std::cmp::min;

use num::clamp;

use rsnano_messages::BlocksAckPayload;

use crate::bootstrap::bootstrapper::Priority;

/// Decides how many blocks to pull
pub(crate) struct PullCountDecider {
    pub max_pull_count: u8,
}

impl PullCountDecider {
    pub fn new(max_pull_count: u8) -> Self {
        Self { max_pull_count }
    }

    pub fn pull_count(&self, priority: Priority) -> u8 {
        // Decide how many blocks to request
        const MIN_PULL_COUNT: u8 = 8;

        let pull_count = clamp(
            f64::from(priority) as u8,
            MIN_PULL_COUNT,
            BlocksAckPayload::MAX_BLOCKS,
        );

        // Limit the max number of blocks to pull
        min(pull_count, self.max_pull_count) as u8
    }
}

impl Default for PullCountDecider {
    fn default() -> Self {
        Self::new(BlocksAckPayload::MAX_BLOCKS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_count() {
        assert_pull_count(Priority::ZERO, 8);
    }

    #[test]
    fn priority_equals_pull_count() {
        assert_pull_count(Priority::new(10.1), 10);
        assert_pull_count(Priority::new(10.9), 10);
        assert_pull_count(Priority::new(11.0), 11);
    }

    #[test]
    fn max() {
        assert_pull_count(Priority::new(999.0), BlocksAckPayload::MAX_BLOCKS as u8);
    }

    #[test]
    fn configured_max() {
        let max = 5;
        let decider = PullCountDecider::new(max);
        assert_eq!(decider.pull_count(Priority::new(999.0)), max);
    }

    fn assert_pull_count(priority: Priority, expected: u8) {
        let decider = PullCountDecider::default();
        assert_eq!(decider.pull_count(priority), expected);
    }
}
