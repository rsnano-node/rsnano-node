use std::collections::BTreeMap;

use rustc_hash::{FxHashMap, FxHashSet};

use rsnano_types::Account;
use rsnano_utils::container_info::{ContainerInfo, ContainerInfoProvider};

use super::priority::{Priority, PriorityKeyDesc};

/// Queue of bootstrapping accounts that are ready to download blocks
#[derive(Default)]
pub(super) struct DownloadQueue {
    by_priority: BTreeMap<PriorityKeyDesc, FxHashSet<Account>>, // descending
    by_account: FxHashMap<Account, Priority>,
}

impl DownloadQueue {
    pub fn len(&self) -> usize {
        self.by_account.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_account.is_empty()
    }

    pub fn contains(&self, account: &Account) -> bool {
        self.by_account.contains_key(account)
    }

    pub fn insert(&mut self, account: Account, priority: Priority) {
        let inserted = self
            .by_priority
            .entry(priority.into())
            .or_default()
            .insert(account);
        debug_assert!(inserted);
        let old = self.by_account.insert(account, priority);
        debug_assert!(old.is_none());
    }

    pub fn pop_lowest_prio(&mut self) -> Option<Account> {
        let mut last_entry = self.by_priority.last_entry()?;
        let accounts = last_entry.get_mut();
        let account = *accounts.iter().next().unwrap();
        if accounts.len() == 1 {
            last_entry.remove();
        } else {
            accounts.remove(&account);
        }
        self.by_account.remove(&account);
        Some(account)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Priority, &Account)> {
        self.by_priority
            .iter()
            .flat_map(|(prio, accs)| accs.iter().map(|a| (prio.0, a)))
    }

    pub fn remove(&mut self, account: &Account) -> bool {
        let Some(priority) = self.by_account.remove(account) else {
            return false;
        };
        let ids = self.by_priority.get_mut(&priority.into()).unwrap();
        ids.remove(account);
        if ids.is_empty() {
            self.by_priority.remove(&priority.into());
        }
        true
    }

    pub fn change_priority(&mut self, account: &Account, new_prio: Priority) -> bool {
        let Some(current_prio) = self.by_account.get_mut(account) else {
            return false;
        };
        let accounts = self
            .by_priority
            .get_mut(&PriorityKeyDesc(*current_prio))
            .unwrap();
        accounts.remove(account);
        if accounts.is_empty() {
            self.by_priority.remove(&PriorityKeyDesc(*current_prio));
        }
        self.by_priority
            .entry(PriorityKeyDesc(new_prio))
            .or_default()
            .insert(*account);
        *current_prio = new_prio;
        true
    }
}

impl ContainerInfoProvider for DownloadQueue {
    fn container_info(&self) -> ContainerInfo {
        [("accounts", self.by_account.len(), 0)].into()
    }
}
