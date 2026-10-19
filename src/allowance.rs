use crate::events::{AllowancesRevoked, Approve};
use crate::storage_types::DataKey;
use crate::validation::require_positive_amount;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// A live allowance: how much, and until when.
///
/// `#[contracttype]` is what makes this storable: the derive supplies the `Val`
/// conversions `Env::storage` needs, and the pair travels as one atomic value,
/// so a reader can never see an amount from one grant beside an expiration from
/// another.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Allowance {
    pub amount: i128,
    pub expiration_ledger: u32,
}

/// Reads the stored allowance entry, or an empty one.
fn load(e: &Env, from: &Address, spender: &Address) -> Allowance {
    e.storage()
        .persistent()
        .get(&DataKey::Allowance(from.clone(), spender.clone()))
        .unwrap_or(Allowance {
            amount: 0,
            expiration_ledger: 0,
        })
}

/// Whether `stored` has already lapsed.
fn is_expired(e: &Env, stored: &Allowance) -> bool {
    stored.expiration_ledger < e.ledger().sequence()
}

/// The amount `spender` may still move on `from`'s behalf.
///
/// An expired allowance reads as `0` rather than as an error.
pub fn allowance(e: &Env, from: &Address, spender: &Address) -> i128 {
    let stored = load(e, from, spender);
    if is_expired(e, &stored) {
        0
    } else {
        stored.amount
    }
}

/// The ledger the `owner`/`spender` allowance expires on, or 0 once it has.
///
/// An expired allowance reports `0` rather than its stale deadline: the two
/// figures are only ever read together, and a live-looking deadline beside a
/// zero amount is a trap for a client that reads the deadline alone.
pub fn allowance_expiration(e: &Env, from: &Address, spender: &Address) -> u32 {
    let stored = load(e, from, spender);
    if is_expired(e, &stored) {
        0
    } else {
        stored.expiration_ledger
    }
}

/// Every spender `owner` currently holds a live allowance with, in grant order.
pub fn spenders_for(e: &Env, owner: &Address) -> Vec<Address> {
    e.storage()
        .persistent()
        .get(&DataKey::AllowanceSpenders(owner.clone()))
        .unwrap_or_else(|| Vec::new(e))
}

/// Adds `spender` to `owner`'s index unless it is already a member.
///
/// A repeated approval must not append a second entry: the index is what
/// `revoke_all_allowances` walks, and a duplicate would make it report and
/// rewrite a spender twice.
fn add_spender(e: &Env, owner: &Address, spender: &Address) {
    let mut index = spenders_for(e, owner);
    if !index.contains(spender.clone()) {
        index.push_back(spender.clone());
        e.storage()
            .persistent()
            .set(&DataKey::AllowanceSpenders(owner.clone()), &index);
    }
}

/// Removes `spender` from `owner`'s index if it is a member.
fn remove_spender(e: &Env, owner: &Address, spender: &Address) {
    let mut index = spenders_for(e, owner);
    if let Some(position) = index.first_index_of(spender.clone()) {
        index.remove(position);
        e.storage()
            .persistent()
            .set(&DataKey::AllowanceSpenders(owner.clone()), &index);
    }
}

/// Writes an allowance, keeping `owner`'s spender index in step.
///
/// The index is maintained here rather than at the call sites. It is the only
/// way to enumerate an owner's approvals, so a grant that is not indexed cannot
/// be found, audited, or revoked in bulk — and an unindexed grant is exactly the
/// one a user who suspects a compromise cannot shut off. Keeping the write in
/// one function makes it impossible to store an allowance without indexing it.
fn store(e: &Env, owner: &Address, spender: &Address, amount: i128, expiration_ledger: u32) {
    if amount == 0 {
        e.storage()
            .persistent()
            .remove(&DataKey::Allowance(owner.clone(), spender.clone()));
        remove_spender(e, owner, spender);
    } else {
        e.storage().persistent().set(
            &DataKey::Allowance(owner.clone(), spender.clone()),
            &Allowance {
                amount,
                expiration_ledger,
            },
        );
        add_spender(e, owner, spender);
    }
}

/// Authorizes `spender` to move up to `amount` of `owner`'s tokens.
///
/// Overwrites rather than accumulating, and indexes the spender on the way in.
///
/// # Panics
///
/// Panics with `ExpirationInPast` when `amount` is positive and
/// `expiration_ledger` is already behind the current ledger. A zero amount is
/// always accepted, because revoking an allowance has no expiry to get wrong.
pub fn approve(
    e: &Env,
    owner: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    require_non_negative(amount);
    if amount > 0 && expiration_ledger < e.ledger().sequence() {
        panic!(
            "ExpirationInPast: expiration ledger {} is before the current ledger {}",
            expiration_ledger,
            e.ledger().sequence()
        );
    }
    store(e, owner, spender, amount, expiration_ledger);
    Approve {
        from: owner.clone(),
        spender: spender.clone(),
        amount,
        expiration_ledger,
    }
    .publish(e);
}

/// Reduces the `owner`/`spender` allowance by exactly `amount` and returns what
/// is left.
///
/// The live allowance is read through [`allowance`], so an expired grant counts
/// as `0` and fails here. The stored expiration is written back unchanged, so a
/// partial spend does not slide the deadline.
///
/// # Panics
///
/// Panics through [`require_positive_amount`] on a non-positive `amount` — a
/// negative amount would otherwise *increase* the grant, since subtracting it
/// — and with `InsufficientAllowance` when the live allowance is below `amount`.
pub fn consume_allowance(e: &Env, owner: &Address, spender: &Address, amount: i128) -> i128 {
    require_positive_amount(amount);
    let current = allowance(e, owner, spender);
    if current < amount {
        panic!(
            "InsufficientAllowance: {} available, {} required",
            current, amount
        );
    }
    let remaining = current - amount;
    let expiration_ledger = load(e, owner, spender).expiration_ledger;
    store(e, owner, spender, remaining, expiration_ledger);
    remaining
}

/// Zeroes every allowance `owner` granted, in one call, and returns how many were
/// revoked.
///
/// The spender index is the entire point: without it there is no way to
/// enumerate an owner's approvals, so a user who suspects a compromise would
/// have to already know every spender by name. The index is walked and each
/// allowance is zeroed, and the index itself is cleared last.
///
/// # Panics
///
/// Panics if the owner is not the caller; the contract cannot clear someone
/// else's approvals on their behalf.
pub fn revoke_all_allowances(e: &Env, owner: &Address) -> u32 {
    let index = spenders_for(e, owner);
    let revoked = index.len();
    for spender in index.iter() {
        e.storage()
            .persistent()
            .remove(&DataKey::Allowance(owner.clone(), spender.clone()));
    }
    e.storage()
        .persistent()
        .remove(&DataKey::AllowanceSpenders(owner.clone()));
    AllowancesRevoked {
        owner: owner.clone(),
        count: revoked,
    }
    .publish(e);
    revoked
}

/// Rejects a negative `amount`, allowing the zero that means "revoke".
fn require_non_negative(amount: i128) -> () {
    assert!(
        amount >= 0,
        "InvalidAmount: amount must not be negative, got {}",
        amount
    );
}
