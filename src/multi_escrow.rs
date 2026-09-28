//! Multi-beneficiary escrows: one deposit split across many payees.
//!
//! A multi-escrow holds a single `total` of `token` for a list of
//! `beneficiaries`, each owed the parallel entry in `amounts`. Settlement is
//! atomic — either every leg is paid or the whole call reverts — so a
//! beneficiary can never be skipped because a later leg failed.

use crate::events::{MultiEscrowCreated, MultiEscrowReleased};
use crate::storage_types::{DataKey, MultiEscrowRecord, MultiEscrowStatus};
use soroban_sdk::{token, Address, Env, Vec};

/// Number of multi-escrows created so far, which is also the next id.
pub fn count(e: &Env) -> u64 {
    e.storage()
        .persistent()
        .get(&DataKey::MultiEscrowCount)
        .unwrap_or(0)
}

/// Reads a multi-escrow record.
///
/// # Panics
///
/// Panics with `MultiEscrowNotFound` when no record exists at `id`.
pub fn record(e: &Env, id: u64) -> MultiEscrowRecord {
    e.storage()
        .persistent()
        .get(&DataKey::MultiEscrow(id))
        .unwrap_or_else(|| panic!("MultiEscrowNotFound: no multi-escrow with id {}", id))
}

/// Whether the record at `id` has been settled — released or refunded.
///
/// # Panics
///
/// Panics with `MultiEscrowNotFound` when no record exists at `id`.
pub fn is_settled(e: &Env, id: u64) -> bool {
    matches!(
        record(e, id).status,
        MultiEscrowStatus::Released | MultiEscrowStatus::Refunded
    )
}

/// Sends `amount` of `token` out of the contract to `to`.
fn pay(e: &Env, token_address: &Address, to: &Address, amount: i128) {
    token::TokenClient::new(e, token_address).transfer(
        &e.current_contract_address(),
        to,
        &amount,
    );
}

/// Holds `total` of `token` split across `beneficiaries` and returns the new
/// record's id.
///
/// `beneficiaries[i]` is owed `amounts[i]`; the two vectors must be the same
/// length and `amounts` must sum exactly to `total`. A mismatch between the
/// deposit and the shares either strands funds or over-pays, so the sum is
/// checked before anything is stored.
///
/// # Panics
///
/// With `MultiEscrowSumMismatch` when `amounts` does not sum to `total`, with
/// `MultiEscrowEmpty` when there are no beneficiaries, and with whatever the
/// token contract raises when the depositor's balance or the token's own
/// authorization rules are not satisfied.
pub fn create(
    e: &Env,
    depositor: &Address,
    token_address: &Address,
    beneficiaries: Vec<Address>,
    amounts: Vec<i128>,
    total: i128,
) -> u64 {
    depositor.require_auth();

    if beneficiaries.len() != amounts.len() {
        panic!(
            "MultiEscrowLengthMismatch: {} beneficiaries but {} amounts",
            beneficiaries.len(),
            amounts.len()
        );
    }
    if beneficiaries.is_empty() {
        panic!("MultiEscrowEmpty: at least one beneficiary is required");
    }

    let mut sum: i128 = 0;
    for amount in amounts.iter() {
        sum = sum
            .checked_add(amount)
            .unwrap_or_else(|| panic!("MultiEscrowOverflow: amounts overflow i128"));
    }
    if sum != total {
        panic!(
            "MultiEscrowSumMismatch: amounts sum to {} but the deposit is {}",
            sum, total
        );
    }

    // Move the funds before recording the escrow. A token transfer that fails
    // panics and reverts the whole invocation, so there is no window in which
    // a record exists for funds that never arrived.
    pay(e, token_address, depositor, -total);

    let id = count(e);
    let record = MultiEscrowRecord {
        depositor: depositor.clone(),
        token: token_address.clone(),
        beneficiaries: beneficiaries.clone(),
        amounts: amounts.clone(),
        total,
        status: MultiEscrowStatus::Active,
    };
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrow(id), &record);
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrowCount, &(id + 1));

    MultiEscrowCreated {
        depositor: depositor.clone(),
        id,
        total,
    }
    .publish(e);

    id
}

/// Pays every beneficiary their stored amount and marks the record `Released`.
///
/// Settlement is atomic: each leg transfers its stored amount to its
/// beneficiary, and a failure on any leg panics and reverts the whole call, so
/// either everyone is paid or nobody is.
///
/// # Panics
///
/// With `MultiEscrowNotFound` for an unknown id, with `MultiEscrowNotActive`
/// when the record has already been settled, and with whatever the token
/// contract raises on a failed transfer.
pub fn release(e: &Env, caller: &Address, id: u64) {
    let mut record = record(e, id);
    if record.status != MultiEscrowStatus::Active {
        panic!(
            "MultiEscrowNotActive: multi-escrow {} is already {:?}",
            id, record.status
        );
    }
    if &record.depositor != caller && !crate::admin::is_admin(e, caller) {
        panic!(
            "Unauthorized: only the depositor or the admin may release multi-escrow {}",
            id
        );
    }
    caller.require_auth();

    // Pay every leg before marking settled. A panic on any leg reverts the
    // whole invocation, so the record can never be left half-paid.
    for (beneficiary, amount) in record.beneficiaries.iter().zip(record.amounts.iter()) {
        pay(e, &record.token, &beneficiary, amount);
    }

    // Drain the record to zero: every leg was paid, so nothing is still owed.
    let drained = Vec::new(e);
    record.amounts = drained;
    record.status = MultiEscrowStatus::Released;
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrow(id), &record);

    MultiEscrowReleased {
        depositor: record.depositor.clone(),
        id,
        total: record.total,
    }
    .publish(e);
}

/// Returns the whole deposit to the depositor and marks the record `Refunded`.
///
/// # Panics
///
/// With `MultiEscrowNotFound` for an unknown id, with `MultiEscrowNotActive`
/// when the record has already been settled, and with whatever the token
/// contract raises on a failed transfer.
pub fn refund(e: &Env, caller: &Address, id: u64) {
    let mut record = record(e, id);
    if record.status != MultiEscrowStatus::Active {
        panic!(
            "MultiEscrowNotActive: multi-escrow {} is already {:?}",
            id, record.status
        );
    }
    if &record.depositor != caller && !crate::admin::is_admin(e, caller) {
        panic!(
            "Unauthorized: only the depositor or the admin may refund multi-escrow {}",
            id
        );
    }
    caller.require_auth();

    pay(e, &record.token, &record.depositor, record.total);

    record.status = MultiEscrowStatus::Refunded;
    e.storage()
        .persistent()
        .set(&DataKey::MultiEscrow(id), &record);
}
