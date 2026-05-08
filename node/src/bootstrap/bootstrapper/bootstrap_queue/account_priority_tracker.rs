use std::cmp::max;

use rustc_hash::FxHashMap;

use rsnano_types::Account;
use rsnano_utils::container_info::{ContainerInfo, ContainerInfoProvider};

use crate::bootstrap::bootstrapper::Priority;

#[derive(Debug, PartialEq, Eq)]
pub enum PriorityUpResult {
    NotFound,
    Upgraded(Priority, Priority),
    Unchanged,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PriorityDownResult {
    Deprioritized(Priority, Priority),
    /// The priority got too low, so the account got erased
    Removed,
    AccountNotFound,
}

#[derive(PartialEq, Eq)]
enum ChangePriorityResult {
    Updated(Priority, Priority),
    Removed,
    NotFound,
    Unchanged,
}

#[derive(Default)]
pub(super) struct AccountPriorityTracker {
    priorities: FxHashMap<Account, Priority>,
}

impl AccountPriorityTracker {
    pub fn insert(&mut self, account: Account, priority: Priority) -> bool {
        if account.is_zero() {
            return false;
        }
        if self.priorities.contains_key(&account) {
            return false;
        }
        self.priorities.insert(account, priority);
        true
    }

    pub fn priority_up(&mut self, account: &Account) -> PriorityUpResult {
        let result = self.modify_priority(account, |prio| prio.increase());

        match result {
            ChangePriorityResult::NotFound => PriorityUpResult::NotFound,
            ChangePriorityResult::Updated(old, new) => PriorityUpResult::Upgraded(old, new),
            ChangePriorityResult::Removed => {
                unreachable!()
            }
            ChangePriorityResult::Unchanged => PriorityUpResult::Unchanged,
        }
    }

    pub fn priority_up_to(
        &mut self,
        account: &Account,
        new_priority: Priority,
    ) -> PriorityUpResult {
        let result = self.modify_priority(account, |old_prio| max(old_prio, new_priority));

        match result {
            ChangePriorityResult::Updated(old, new) => PriorityUpResult::Upgraded(old, new),
            ChangePriorityResult::Removed => unreachable!(),
            ChangePriorityResult::Unchanged => PriorityUpResult::Unchanged,
            ChangePriorityResult::NotFound => PriorityUpResult::NotFound,
        }
    }

    pub fn priority_down(&mut self, account: &Account) -> PriorityDownResult {
        let change_result = self.modify_priority(account, |prio| prio / Priority::DIVIDE);

        match change_result {
            ChangePriorityResult::Updated(old, new) => PriorityDownResult::Deprioritized(old, new),
            ChangePriorityResult::Removed => PriorityDownResult::Removed,
            ChangePriorityResult::NotFound => PriorityDownResult::AccountNotFound,
            ChangePriorityResult::Unchanged => {
                unreachable!("the account is ether downgraded, removed or not found")
            }
        }
    }

    pub fn contains(&self, account: &Account) -> bool {
        self.priorities.contains_key(account)
    }

    pub fn get(&self, account: &Account) -> Option<Priority> {
        self.priorities.get(account).copied()
    }

    pub fn len(&self) -> usize {
        self.priorities.len()
    }

    pub fn remove(&mut self, account: &Account) -> Option<Priority> {
        self.priorities.remove(account)
    }
}

impl ContainerInfoProvider for AccountPriorityTracker {
    fn container_info(&self) -> ContainerInfo {
        [("priorities", self.priorities.len(), 0)].into()
    }
}

impl AccountPriorityTracker {
    fn modify_priority<F>(&mut self, account: &Account, f: F) -> ChangePriorityResult
    where
        F: Fn(Priority) -> Priority,
    {
        let Some(current_prio) = self.priorities.get_mut(account) else {
            return ChangePriorityResult::NotFound;
        };

        let old_prio = *current_prio;
        let new_prio = f(old_prio);
        if new_prio == old_prio {
            return ChangePriorityResult::Unchanged;
        }

        if new_prio < Priority::CUTOFF {
            self.priorities.remove(account);
            return ChangePriorityResult::Removed;
        }

        *current_prio = new_prio;
        ChangePriorityResult::Updated(old_prio, new_prio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /* priority_up */

    #[test]
    fn priority_up_does_nothing_if_account_not_prioritized() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        let result = tracker.priority_up(&account);
        assert_eq!(result, PriorityUpResult::NotFound);
        assert_eq!(tracker.get(&account), None);
    }

    #[test]
    fn priority_up_upgrades_existing_account() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        tracker.priority_up(&account);
        let result = tracker.priority_up(&account);
        let expected_new = Priority::INITIAL.increase();
        assert_eq!(
            result,
            PriorityUpResult::Upgraded(Priority::INITIAL, expected_new)
        );
        assert_eq!(tracker.get(&account), Some(expected_new));
    }

    #[test]
    fn priority_up_returns_unchanged_at_max() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        for _ in 0..100 {
            tracker.priority_up(&account);
        }
        assert_eq!(tracker.get(&account), Some(Priority::MAX));
        assert_eq!(tracker.priority_up(&account), PriorityUpResult::Unchanged);
    }

    /* priority_down */

    #[test]
    fn priority_down_decreases_priority() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        tracker.priority_up_to(&account, Priority::INITIAL);
        let result = tracker.priority_down(&account);
        let expected = Priority::INITIAL / Priority::DIVIDE;
        assert_eq!(
            result,
            PriorityDownResult::Deprioritized(Priority::INITIAL, expected)
        );
        assert_eq!(tracker.get(&account), Some(expected));
    }

    #[test]
    fn priority_down_returns_not_found_for_unknown_account() {
        let mut tracker = AccountPriorityTracker::default();
        assert_eq!(
            tracker.priority_down(&Account::from(1)),
            PriorityDownResult::AccountNotFound
        );
    }

    #[test]
    fn priority_down_removes_account_when_below_cutoff() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        tracker.insert(account, Priority::INITIAL);
        let result = loop {
            let r = tracker.priority_down(&account);
            if r == PriorityDownResult::Removed {
                break r;
            }
        };
        assert_eq!(result, PriorityDownResult::Removed);
        assert!(!tracker.contains(&account));
    }

    /* contains / get / len / remove */

    #[test]
    fn contains_and_len() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        assert!(!tracker.contains(&account));
        assert_eq!(tracker.len(), 0);
        tracker.priority_up(&account);
        assert!(tracker.contains(&account));
        assert_eq!(tracker.len(), 1);
    }

    #[test]
    fn remove_returns_priority_and_erases_account() {
        let mut tracker = AccountPriorityTracker::default();
        let account = Account::from(1);
        tracker.priority_up(&account);
        assert_eq!(tracker.remove(&account), Some(Priority::INITIAL));
        assert!(!tracker.contains(&account));
        assert_eq!(tracker.remove(&account), None);
    }
}
