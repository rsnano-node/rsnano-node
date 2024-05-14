use rsnano_core::{Amount, BlockEnum};

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

/**
 * Tag for the type of the election status
 */
#[repr(u8)]
#[derive(PartialEq, Eq, Debug, Clone, Copy, FromPrimitive)]
pub enum ElectionStatusType {
    Ongoing = 0,
    ActiveConfirmedQuorum = 1,
    ActiveConfirmationHeight = 2,
    InactiveConfirmationHeight = 3,
    Stopped = 5,
}

/// Information on the status of an election
#[derive(Clone)]
pub struct ElectionStatus {
    pub winner: Option<Arc<BlockEnum>>,
    pub tally: Amount,
    pub final_tally: Amount,
    pub confirmation_request_count: u32,
    pub block_count: u32,
    pub voter_count: u32,
    pub election_end: SystemTime,
    pub election_duration: Duration,
    pub election_status_type: ElectionStatusType,
}

impl Default for ElectionStatus {
    fn default() -> Self {
        Self {
            winner: None,
            tally: Amount::zero(),
            final_tally: Amount::zero(),
            block_count: 0,
            voter_count: 0,
            confirmation_request_count: 0,
            election_end: SystemTime::now(),
            election_duration: Duration::ZERO,
            election_status_type: ElectionStatusType::InactiveConfirmationHeight,
        }
    }
}