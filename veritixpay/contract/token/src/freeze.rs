use crate::storage_types::{DataKey, PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD};
use soroban_sdk::{symbol_short, vec, Address, Env, Vec};

pub fn is_frozen(e: &Env, addr: &Address) -> bool {
    let key = DataKey::Freeze(addr.clone());
    let storage = e.storage().persistent();
    let frozen = storage.get(&key).unwrap_or(false);
    if frozen {
        storage.extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }
    frozen
}

pub fn freeze_account(e: &Env, admin: Address, target: Address) {
    let stored_admin = crate::admin::read_admin(e);
    if target == stored_admin {
        panic!("InvalidFreeze: cannot freeze the admin address");
    }
    let key = DataKey::Freeze(target.clone());
    let storage = e.storage().persistent();
    if storage.get::<DataKey, bool>(&key).unwrap_or(false) {
        panic!("AlreadyFrozen: account is already frozen");
    }
    storage.set(&key, &true);
    storage.extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

    // Maintain the enumerable frozen-accounts list (idempotent — no duplicate adds).
    let list_key = DataKey::FrozenAccounts;
    let mut list: Vec<Address> = storage.get(&list_key).unwrap_or_else(|| vec![e]);
    if !list.contains(&target) {
        list.push_back(target.clone());
        storage.set(&list_key, &list);
        storage.extend_ttl(&list_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }

    e.events().publish((symbol_short!("frozen"), target), admin);
}

pub fn unfreeze_account(e: &Env, admin: Address, target: Address) {
    if !e.storage().persistent().get::<DataKey, bool>(&DataKey::Freeze(target.clone())).unwrap_or(false) {
        panic!("NotFrozen: account is not frozen");
    }
    e.storage()
        .persistent()
        .remove(&DataKey::Freeze(target.clone()));

    // Remove from the enumerable list.
    let list_key = DataKey::FrozenAccounts;
    let storage = e.storage().persistent();
    if let Some(mut list) = storage.get::<DataKey, Vec<Address>>(&list_key) {
        if let Some(idx) = list.iter().position(|a| a == target) {
            list.remove(idx as u32);
            storage.set(&list_key, &list);
            storage.extend_ttl(&list_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        }
    }

    e.events()
        .publish((symbol_short!("unfrozen"), target), admin);
}

pub fn get_frozen_accounts(e: &Env) -> Vec<Address> {
    let storage = e.storage().persistent();
    let key = DataKey::FrozenAccounts;
    let list: Vec<Address> = storage.get(&key).unwrap_or_else(|| vec![e]);
    if !list.is_empty() {
        storage.extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }
    list
}
