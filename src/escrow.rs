use crate::events::{EscrowCreated, EscrowRefunded, EscrowReleased};
use crate::storage_types::{DataKey, EscrowRecord, EscrowStatus};
use crate::validation::require_positive_amount;
use soroban_sdk::{token, Address, Env};

/// Number of escrows created so far, which is also the next escrow id.
///
/// Ids are handed out by incrementing this counter rather than by deriving
/// anything from the call, so an id is never reused and a settled escrow can
/// never be confused with a live one that happens to sit at the same storage
/// offset.
pub fn count(e: &Env) -> u32 {
    e.storage().persistent().get(&DataKey::EscrowCount).unwrap_or(0)
}

/// Total token amount still held across all active escrows.
pub fn value_locked(e: &Env) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::EscrowValueLocked)
        .unwrap_or(0)
}

/// Stores an escrow record under the next id and advances the counter past it.
fn store_new(e: &Env, record: &EscrowRecord) -> u32 {
    let id = count(e);
    e.storage().persistent().set(&DataKey::EscrowRecord(id), record);
    e.storage().persistent().set(&DataKey::EscrowCount, &id + 1);
    id
}

/// Overwrites the record stored at `id`.
fn store_record(e: &Env, id: u32, record: &EscrowRecord) {
    e.storage().persistent().set(&DataKey::EscrowRecord(id), record);
}

/// Adds `amount` to the total held in active escrows.
fn increase_value_locked(e: &Env, amount: i128) {
    let new_total = value_locked(e)
        .checked_add(amount)
        .unwrap_or_else(|| panic!("EscrowOverflow: locked value would overflow i128"));
    e.storage()
        .persistent()
        .set(&DataKey::EscrowValueLocked, &new_total);
}

/// Takes `amount` out of the total held in active escrows, refusing to go
/// negative.
///
/// A settlement that could drive the total below zero would mean the contract
/// paid out more than it was holding, and every caller that could reach it is
/// one a user has already trusted with the escrow. The subtraction is checked
/// rather than saturating so a bug surfaces here instead of as a silently wrong
/// total.
fn decrease_value_locked(e: &Env, amount: i128) {
    let current = value_locked(e);
    let new_total = current.checked_sub(amount).unwrap_or_else(|| {
        panic!(
            "EscrowUnderflow: paying out {} would exceed the {} still held",
            amount, current
        )
    });
    e.storage()
        .persistent()
        .set(&DataKey::EscrowValueLocked, &new_total);
}

/// Reads an escrow record.
///
/// # Panics
///
/// Panics with `EscrowNotFound` when no escrow exists at `id`. A client asking
/// about an escrow that was never created should hear that, rather than a
/// plausible-looking default.
pub fn record(e: &Env, id: u32) -> EscrowRecord {
    e.storage()
        .persistent()
        .get(&DataKey::EscrowRecord(id))
        .unwrap_or_else(|| panic!("EscrowNotFound: no escrow with id {}", id))
}

/// Whether `status` is terminal.
pub fn is_settled_status(status: &EscrowStatus) -> bool {
    matches!(status, EscrowStatus::Released | EscrowStatus::Refunded)
}

/// Whether the escrow at `id` has been settled.
///
/// # Panics
///
/// Panics with `EscrowNotFound` when no escrow exists at `id`, for the same
/// reason [`record`] does: "not settled" would be a false answer to a question
/// about an escrow that does not exist, and a client would carry that forward.
pub fn is_settled(e: &Env, id: u32) -> bool {
    is_settled_status(&record(e, id).status)
}

/// Panics while an open dispute freezes the escrow.
///
/// A buyer who did not get into the event must be able to stop the organizer
/// being paid: once `raise_dispute` opens a dispute over the escrow, neither
/// release nor refund may move funds until a ruling is final.
fn require_no_open_dispute(e: &Env, id: u32) {
    if crate::dispute::is_dispute_open(e, id as u64) {
        panic!(
            "EscrowDisputed: escrow {} is frozen by an open dispute",
            id
        );
    }
}

/// Panics unless `record` is still `Active`.
fn require_active(id: u32, record: &EscrowRecord) {
    if record.status == EscrowStatus::Disputed {
        panic!("EscrowDisputed: escrow {} is frozen by an open dispute", id);
    }
    if is_settled_status(&record.status) {
        panic!(
            "EscrowNotActive: escrow {} is already {:?}",
            id, record.status
        );
    }
}

/// Requires that `caller` may settle `record`.
///
/// Either the depositor — who paid the money and is entitled to take it back —
/// or the contract admin, who is the backstop when the two parties cannot
/// agree. The check comes before the state is read as final and before any
/// transfer, so an unauthorized caller cannot move a token.
fn require_settlement_authority(e: &Env, caller: &Address, record: &EscrowRecord) {
    if &record.depositor != caller && !crate::admin::is_admin(e, caller) {
        panic!(
            "Unauthorized: only the depositor or the admin may settle escrow for {:?}",
            record.beneficiary
        );
    }
    caller.require_auth();
}

/// Sends `amount` of `token` out of the contract to `to`.
fn pay(e: &Env, token_address: &Address, to: &Address, amount: i128) {
    token::TokenClient::new(e, token_address).transfer(
        &e.current_contract_address(),
        to,
        &amount,
    );
}

/// Holds `amount` of `token` for `beneficiary` and returns the new escrow's id.
///
/// Funds leave the depositor's account immediately and only reach the
/// beneficiary once the event completes, so the contract — not the depositor —
/// holds them in the meantime.
///
/// The transfer goes through the token's own SEP-41 client rather than this
/// contract's internal ledger, because the escrowed token is a separate
/// contract: a caller may escrow a token this contract knows nothing about.
///
/// # Panics
///
/// Panics through [`require_positive_amount`] on a non-positive `amount`, and
/// propagates whatever the token contract raises when the depositor's balance
/// or the token's own authorization rules are not satisfied.
pub fn create(
    e: &Env,
    depositor: &Address,
    beneficiary: &Address,
    token_address: &Address,
    amount: i128,
    deadline_ledger: u32,
) -> u32 {
    require_positive_amount(amount);
    depositor.require_auth();

    // Move the funds before recording the escrow. A token transfer that fails
    // panics and reverts the whole invocation, so there is no window in which
    // an escrow exists for funds that never arrived.
    pay(e, token_address, depositor, -amount);
    let id = store_new(
        e,
        &EscrowRecord {
            depositor: depositor.clone(),
            beneficiary: beneficiary.clone(),
            token: token_address.clone(),
            amount,
            deadline_ledger,
            status: EscrowStatus::Active,
        },
    );
    increase_value_locked(e, amount);

    EscrowCreated {
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        id,
        token: token_address.clone(),
        amount,
        deadline_ledger,
    }
    .publish(e);

    id
}

/// Pays the beneficiary everything still held and closes the escrow as
/// `Released`.
pub fn release(e: &Env, caller: &Address, id: u32) {
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    require_settlement_authority(e, caller, &record);

    let amount = record.amount;
    pay(e, &record.token, &record.beneficiary, amount);
    decrease_value_locked(e, amount);
    record.status = EscrowStatus::Released;
    record.amount = 0;
    store_record(e, id, &record);

    EscrowReleased {
        beneficiary: record.beneficiary.clone(),
        id,
        amount,
        remaining: 0,
    }
    .publish(e);
}

/// Returns everything still held to the depositor and closes the escrow as
/// `Refunded`.
///
/// # Panics
///
/// Panics with `EscrowNotActive` when the escrow has already been settled, so a
/// refund can never race a release into paying the same funds twice.
pub fn refund(e: &Env, caller: &Address, id: u32) {
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    require_settlement_authority(e, caller, &record);

    let amount = record.amount;
    pay(e, &record.token, &record.depositor, amount);
    decrease_value_locked(e, amount);
    record.status = EscrowStatus::Refunded;
    record.amount = 0;
    store_record(e, id, &record);

    EscrowRefunded {
        depositor: record.depositor.clone(),
        id,
        amount,
    }
    .publish(e);
}

/// Pays `amount` of the held funds to the beneficiary without closing the
/// escrow, unless it was the last of the money.
///
/// A partially-delivered event is settled over several calls; the record's
/// `amount` is the remainder, and the escrow becomes `Released` on the call that
/// takes it to zero. That keeps "is anything still owed" a single field read
/// rather than a sum over the event log.
///
/// # Panics
///
/// Panics through [`require_positive_amount`] on a non-positive `amount`, with
/// `EscrowNotActive` when the escrow is already settled, with
/// `AmountAboveRemainder` when `amount` exceeds what is still held, and with
/// whatever the token contract raises on a failed transfer.
pub fn release_partial(e: &Env, caller: &Address, id: u32, amount: i128) {
    require_positive_amount(amount);
    require_no_open_dispute(e, id);
    let mut record = record(e, id);
    require_active(id, &record);
    require_settlement_authority(e, caller, &record);
    if amount > record.amount {
        panic!(
            "AmountAboveRemainder: {} requested, {} remaining",
            amount, record.amount
        );
    }

    pay(e, &record.token, &record.beneficiary, amount);
    decrease_value_locked(e, amount);
    let remaining = record.amount - amount;
    record.amount = remaining;
    if remaining == 0 {
        record.status = EscrowStatus::Released;
    }
    store_record(e, id, &record);

    EscrowReleased {
        beneficiary: record.beneficiary.clone(),
        id,
        amount,
        remaining,
    }
    .publish(e);
}
