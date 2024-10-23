use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::mem::size_of;

use crate::Root;

#[derive(Clone, Copy, FromPrimitive, PartialEq, Eq)]
pub enum WorkVersion {
    Unspecified,
    Work1,
}

impl WorkVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkVersion::Work1 => "work_1",
            WorkVersion::Unspecified => "unspecified",
        }
    }
}

impl TryFrom<u8> for WorkVersion {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(WorkVersion::Unspecified),
            1 => Ok(WorkVersion::Work1),
            _ => Err(anyhow!("unknown work version")),
        }
    }
}

pub trait Difficulty: Send + Sync {
    fn get_difficulty(&self, root: &Root, work: u64) -> u64;
    fn clone(&self) -> Box<dyn Difficulty>;
}

#[derive(Clone, Default)]
pub struct DifficultyV1 {}
impl DifficultyV1 {
    pub fn to_multiplier(difficulty: u64, base_difficulty: u64) -> f64 {
        debug_assert!(difficulty > 0);
        base_difficulty.wrapping_neg() as f64 / difficulty.wrapping_neg() as f64
    }

    pub fn from_multiplier(multiplier: f64, base_difficulty: u64) -> u64 {
        debug_assert!(multiplier > 0f64);
        let reverse_difficulty: u128 =
            ((base_difficulty.wrapping_neg() as f64) / multiplier) as u128;
        if reverse_difficulty > u64::MAX as u128 {
            0
        } else if reverse_difficulty != 0 || base_difficulty == 0 || multiplier < 1f64 {
            (reverse_difficulty as u64).wrapping_neg()
        } else {
            u64::MAX
        }
    }
}

impl Difficulty for DifficultyV1 {
    fn get_difficulty(&self, root: &Root, work: u64) -> u64 {
        let mut buffer = [0; size_of::<u64>()];
        let mut hasher = Blake2bVar::new(buffer.len()).unwrap();
        hasher.update(&work.to_le_bytes());
        hasher.update(root.as_bytes());
        hasher.finalize_variable(&mut buffer).unwrap();
        u64::from_le_bytes(buffer)
    }

    fn clone(&self) -> Box<dyn Difficulty> {
        Box::<DifficultyV1>::default()
    }
}

#[derive(Clone, Default)]
pub struct StubDifficulty {
    preset_difficulties: HashMap<(Root, u64), u64>,
}

impl StubDifficulty {
    pub fn new() -> Self {
        Self {
            preset_difficulties: HashMap::new(),
        }
    }

    pub fn set_difficulty(&mut self, root: Root, work: u64, difficulty: u64) {
        self.preset_difficulties.insert((root, work), difficulty);
    }
}

impl Difficulty for StubDifficulty {
    fn get_difficulty(&self, root: &Root, work: u64) -> u64 {
        self.preset_difficulties
            .get(&(*root, work))
            .cloned()
            .unwrap_or(work)
    }

    fn clone(&self) -> Box<dyn Difficulty> {
        Box::new(StubDifficulty {
            preset_difficulties: self.preset_difficulties.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_difficulty() {
        let mut difficulty = StubDifficulty::new();
        assert_eq!(difficulty.get_difficulty(&Root::from(1), 2), 2);

        difficulty.set_difficulty(Root::from(1), 2, 3);
        assert_eq!(difficulty.get_difficulty(&Root::from(1), 2), 3);
    }

    #[test]
    fn difficulty_for_root() {
        let difficulty = DifficultyV1::default();
        let result = difficulty.get_difficulty(&Root::from(123), 456);
        assert_eq!(result, 10978371542656683347);
    }

    #[test]
    fn multipliers_1() {
        let base = 0xff000000_00000000_u64;
        let difficulty = 0xfff27e7a_57c285cd_u64;
        let expected_multiplier = 18.95461493377003_f64;

        assert!(
            (expected_multiplier - DifficultyV1::to_multiplier(difficulty, base)).abs() < 1e-10
        );
        assert_eq!(
            difficulty,
            DifficultyV1::from_multiplier(expected_multiplier, base)
        );
    }

    #[test]
    fn multipliers_2() {
        let base = 0xffffffc0_00000000_u64;
        let difficulty = 0xfffffe00_00000000_u64;
        let expected_multiplier = 0.125_f64;

        assert!(
            (expected_multiplier - DifficultyV1::to_multiplier(difficulty, base)).abs() < 1e-10
        );
        assert_eq!(
            difficulty,
            DifficultyV1::from_multiplier(expected_multiplier, base)
        );
    }

    #[test]
    fn multipliers_3() {
        let base = u64::MAX;
        let difficulty = 0xffffffff_ffffff00_u64;
        let expected_multiplier = 0.00390625_f64;

        assert!((expected_multiplier - DifficultyV1::to_multiplier(difficulty, base)) < 1e-10);
        assert_eq!(
            difficulty,
            DifficultyV1::from_multiplier(expected_multiplier, base)
        );
    }

    #[test]
    fn multipliers_4() {
        let base = 0x80000000_00000000_u64;
        let difficulty = 0xf0000000_00000000_u64;
        let expected_multiplier = 8.0_f64;

        assert!((expected_multiplier - DifficultyV1::to_multiplier(difficulty, base)) < 1e-10);
        assert_eq!(
            difficulty,
            DifficultyV1::from_multiplier(expected_multiplier, base)
        );
    }

    // The death checks don't fail on a release config, so guard against them
    #[cfg(debug_assertions)]
    #[test]
    fn multipliers_nil() {
        let base = 0xffffffc0_00000000_u64;
        let difficulty_nil = 0_u64;
        let multiplier_nil = 0_f64;

        assert!(
            std::panic::catch_unwind(|| { DifficultyV1::to_multiplier(difficulty_nil, base) })
                .is_err()
        );
        assert!(std::panic::catch_unwind(|| {
            DifficultyV1::from_multiplier(multiplier_nil, base)
        })
        .is_err());
    }

    #[test]
    fn difficulty_overflow_max() {
        // Overflow max (attempt to overflow & receive lower difficulty)

        let base = u64::MAX; // Max possible difficulty
        let difficulty = u64::MAX;
        let multiplier = 1.001_f64; // Try to increase difficulty above max

        assert_eq!(difficulty, DifficultyV1::from_multiplier(multiplier, base));
    }

    #[test]
    fn difficulty_overflow_min() {
        // Overflow min (attempt to overflow & receive higher difficulty)

        let base = 1_u64; // Min possible difficulty before 0
        let difficulty = 0_u64;
        let multiplier = 0.999_f64; // Increase difficulty

        assert_eq!(difficulty, DifficultyV1::from_multiplier(multiplier, base));
    }

    #[test]
    fn difficulty_0_decrease() {
        // Tests with base difficulty 0 should return 0 with any multiplier
        let base = 0_u64; // Min possible difficulty
        let difficulty = 0_u64;
        let multiplier = 0.000000001_f64; // Decrease difficulty

        assert_eq!(difficulty, DifficultyV1::from_multiplier(multiplier, base));
    }

    #[test]
    fn difficulty_0_increase() {
        let base = 0_u64; // Min possible difficulty
        let difficulty = 0_u64;
        let multiplier = 1000000000.0_f64; // Increase difficulty

        assert_eq!(difficulty, DifficultyV1::from_multiplier(multiplier, base));
    }
}
