use rsnano_core::{Account, Amount};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountsWithAmountsDto {
    pub accounts: HashMap<Account, Amount>,
}

impl AccountsWithAmountsDto {
    pub fn new(accounts: HashMap<Account, Amount>) -> Self {
        Self { accounts }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RepresentativesDto {
    pub representatives: HashMap<Account, Amount>,
}

impl RepresentativesDto {
    pub fn new(representatives: HashMap<Account, Amount>) -> Self {
        Self { representatives }
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DelegatorsDto {
    pub delegators: HashMap<Account, Amount>,
}

impl DelegatorsDto {
    pub fn new(delegators: HashMap<Account, Amount>) -> Self {
        Self { delegators }
    }
}
