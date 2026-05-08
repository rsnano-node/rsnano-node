use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rsnano_nullable_clock::Timestamp;
use rsnano_types::{Account, BlockHash};
use rsnano_utils::container_info::{ContainerInfo, ContainerInfoProvider};

use rustc_hash::{FxHashMap, FxHashSet};

/// A blocked account is an account that has failed to insert a new block because the source block is not currently present in the ledger
/// An account is unblocked once it has a block successfully inserted
#[derive(Default)]
pub(super) struct BlockedAccounts {
    /// account => dep block + dep account + blocked_at
    by_account: FxHashMap<Account, (BlockHash, Option<Account>, Timestamp)>,
    by_dependency: BTreeMap<BlockHash, Vec<Account>>,
    by_dependency_account: BTreeMap<Account, FxHashSet<Account>>,
    by_timestamp: BTreeSet<(Timestamp, Account)>,
    requested_dependencies: FxHashMap<BlockHash, Timestamp>,
    requested_timestamps: VecDeque<(BlockHash, Timestamp)>,
}

impl BlockedAccounts {
    pub fn len(&self) -> usize {
        self.by_account.len()
    }

    pub(crate) fn known_dependencies(&self) -> usize {
        self.by_dependency_account
            .range(Account::from(1)..)
            .map(|(_, accs)| accs.len())
            .sum()
    }

    pub(crate) fn unique_dependency_accounts(&self) -> usize {
        let mut known = self.by_dependency_account.len();
        if self.by_dependency_account.contains_key(&Account::ZERO) {
            known -= 1;
        }
        known
    }

    pub fn missing_sends(&self) -> impl Iterator<Item = &BlockHash> {
        self.by_dependency.keys()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, account: Account, dependency: BlockHash, timestamp: Timestamp) {
        debug_assert!(!account.is_zero());
        let old = self
            .by_account
            .insert(account, (dependency, None, timestamp));
        debug_assert!(old.is_none());
        self.by_dependency
            .entry(dependency)
            .or_default()
            .push(account);
        let inserted = self
            .by_dependency_account
            .entry(Account::ZERO)
            .or_default()
            .insert(account);
        debug_assert!(inserted);
        let inserted = self.by_timestamp.insert((timestamp, account));
        debug_assert!(inserted);
    }

    pub fn count_by_dependency_account(&self, dep_account: &Account) -> usize {
        self.by_dependency_account
            .get(dep_account)
            .map(|accs| accs.len())
            .unwrap_or_default()
    }

    pub fn next_unknown_blocking_hash(&self) -> Option<BlockHash> {
        // Scan all entries with unknown dependency account
        let accounts = self.by_dependency_account.get(&Account::ZERO)?;
        accounts.iter().find_map(|account| {
            let (dep_block, _, _) = self.by_account.get(account).unwrap();
            if self.requested_dependencies.contains_key(dep_block) {
                None
            } else {
                Some(*dep_block)
            }
        })
    }

    pub fn dependency_account_requested(&mut self, dependency: &BlockHash, now: Timestamp) {
        self.requested_dependencies.insert(*dependency, now);
        self.requested_timestamps.push_back((*dependency, now));
    }

    pub fn dependency_account_not_found(&mut self, dependency: &BlockHash) {
        self.requested_dependencies.remove(dependency);
    }

    pub fn modify_dependency_account(
        &mut self,
        dependency: &BlockHash,
        new_dependency_account: Account,
    ) -> Vec<Account> {
        let mut updated = Vec::new();
        self.requested_dependencies.remove(dependency);
        let Some(accounts) = self.by_dependency.get(dependency) else {
            return updated;
        };

        for account in accounts {
            let (_, dep_account, _) = self.by_account.get_mut(account).unwrap();
            if *dep_account != Some(new_dependency_account) {
                let old_dependency_account = dep_account.unwrap_or_default();
                *dep_account = Some(new_dependency_account);
                let old = self
                    .by_dependency_account
                    .get_mut(&old_dependency_account)
                    .unwrap();
                if old.len() == 1 {
                    self.by_dependency_account.remove(&old_dependency_account);
                } else {
                    old.remove(account);
                }
                self.by_dependency_account
                    .entry(new_dependency_account)
                    .or_default()
                    .insert(*account);

                updated.push(*account);
            }
        }

        updated
    }

    pub fn get_info(&self, account: &Account) -> Option<(BlockHash, Option<Account>)> {
        let (dep_block, dep_account, _) = self.by_account.get(account)?;
        Some((*dep_block, *dep_account))
    }

    /// Iterates over `(dependency_account, blocked_account)` pairs for all blocked
    /// accounts whose dependency account is known (i.e. non-zero).
    pub fn iter_known_dep_accounts(&self) -> impl Iterator<Item = (Account, &Account)> {
        self.by_dependency_account
            .range(Account::from(1)..)
            .flat_map(|(dep_account, accs)| {
                let dep = *dep_account;
                accs.iter()
                    .map(move |blocked_account| (dep, blocked_account))
            })
    }

    pub fn iter_by_timestamp(&self) -> impl Iterator<Item = &Account> {
        self.by_timestamp.iter().map(|(_, account)| account)
    }

    pub fn oldest(&self) -> Option<&Account> {
        self.by_timestamp.first().map(|(_, account)| account)
    }

    /// Removes entries older than the given cutoff and all entries dependent on them
    /// Returns the number of removed entries
    pub fn remove_older_than(&mut self, cutoff: Timestamp) -> Vec<Account> {
        let mut removed = Vec::new();
        while let Some((timestamp, account)) = self.by_timestamp.first() {
            if *timestamp >= cutoff {
                // Entries are sorted by timestamp, no need to continue
                break;
            }
            let account = *account;
            removed.extend(self.remove_account_and_dependents(&account));
        }

        removed
    }

    pub fn remove_account_and_dependents(&mut self, account: &Account) -> Vec<Account> {
        let mut stack = vec![*account];
        let mut removed = Vec::new();

        while let Some(account) = stack.pop() {
            if !self.remove(&account) {
                continue;
            }

            removed.push(account);

            if let Some(dependents) = self.by_dependency_account.get(&account) {
                stack.extend(dependents.iter());
            }
        }

        removed
    }

    pub fn remove(&mut self, account: &Account) -> bool {
        let Some((dep_block, dep_account, blocked_at)) = self.by_account.remove(account) else {
            return false;
        };
        self.requested_dependencies.remove(&dep_block);

        let dep_accounts = self.by_dependency.get_mut(&dep_block).unwrap();
        if dep_accounts.len() > 1 {
            dep_accounts.retain(|i| i != account);
        } else {
            self.by_dependency.remove(&dep_block);
        }

        let dep_account = dep_account.unwrap_or_default();
        let dep_accounts = self.by_dependency_account.get_mut(&dep_account).unwrap();
        if dep_accounts.len() > 1 {
            dep_accounts.remove(account);
        } else {
            self.by_dependency_account.remove(&dep_account);
        }

        let removed = self.by_timestamp.remove(&(blocked_at, *account));
        debug_assert!(removed);

        true
    }

    pub fn remove_requests_older_than(&mut self, cutoff: Timestamp) {
        while let Some((_, started)) = self.requested_timestamps.front()
            && *started < cutoff
        {
            let (hash, started) = self.requested_timestamps.pop_front().unwrap();
            if let Some(t) = self.requested_dependencies.get(&hash)
                && *t == started
            {
                self.requested_dependencies.remove(&hash);
            }
        }
    }

    pub fn contains(&self, account: &Account) -> bool {
        self.by_account.contains_key(account)
    }
}

impl ContainerInfoProvider for BlockedAccounts {
    fn container_info(&self) -> ContainerInfo {
        [
            ("accounts", self.by_account.len(), 0),
            ("by_dependency", self.by_dependency.len(), 0),
            ("by_dependency_account", self.by_dependency_account.len(), 0),
            ("by_timestamp", self.by_timestamp.len(), 0),
            (
                "requested_dependencies",
                self.requested_dependencies.len(),
                0,
            ),
        ]
        .into()
    }
}
