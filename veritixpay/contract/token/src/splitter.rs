//! Splitter module.
//! Implements BPS-based multi-recipient distributions with deterministic last-recipient dust handling.
//! Distribution is caller-authenticated by sender to avoid unauthorized payout triggers.

use crate::balance::{receive_balance, spend_balance};
use crate::storage_types::{
    increment_counter, read_counter, read_persistent_record, write_persistent_record, DataKey,
    SPLIT_BUMP_AMOUNT, SPLIT_LIFETIME_THRESHOLD,
};
use crate::validation::require_positive_amount;
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecipient {
    pub address: Address,
    pub share_bps: u32, // 10000 bps = 100%
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecord {
    pub id: u32,
    pub sender: Address,
    pub recipients: Vec<SplitRecipient>,
    pub total_amount: i128,
    pub distributed: bool,
    pub cancelled: bool,
    pub memo: Bytes,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitterStats {
    pub total_splits: u32,
    pub distributed_count: u32,
    pub cancelled_count: u32,
    pub total_distributed_value: i128,
}

pub fn splitter_stats(e: &Env) -> SplitterStats {
    SplitterStats {
        total_splits: read_counter(e, &DataKey::SplitCount),
        distributed_count: read_counter(e, &DataKey::DistributedCount),
        cancelled_count: read_counter(e, &DataKey::CancelledCount),
        total_distributed_value: e
            .storage()
            .instance()
            .get(&DataKey::TotalDistributedValue)
            .unwrap_or(0i128),
    }
}

fn append_recipient_split_index(e: &Env, recipient: &Address, split_id: u32) {
    let key = DataKey::RecipientSplits(recipient.clone());
    let mut ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(e));

    let mut exists = false;
    for id in ids.iter() {
        if id == split_id {
            exists = true;
            break;
        }
    }
    if !exists {
        ids.push_back(split_id);
    }

    e.storage().persistent().set(&key, &ids);
    e.storage()
        .persistent()
        .extend_ttl(&key, SPLIT_LIFETIME_THRESHOLD, SPLIT_BUMP_AMOUNT);
}

fn remove_recipient_split_index(e: &Env, recipient: &Address, split_id: u32) {
    let key = DataKey::RecipientSplits(recipient.clone());
    let ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(e));
    let mut updated = Vec::new(e);

    for id in ids.iter() {
        if id != split_id {
            updated.push_back(id);
        }
    }

    e.storage().persistent().set(&key, &updated);
    e.storage()
        .persistent()
        .extend_ttl(&key, SPLIT_LIFETIME_THRESHOLD, SPLIT_BUMP_AMOUNT);
}

pub fn create_split(
    e: &Env,
    sender: Address,
    recipients: Vec<SplitRecipient>,
    total_amount: i128,
) -> u32 {
    require_positive_amount(total_amount);

    // 1. Reject empty recipient list
    if recipients.is_empty() {
        panic!("recipients list cannot be empty");
    }

    // Cap at 20 recipients to stay within Soroban per-tx computational and
    // ledger entry limits. Exceeding this could cause mid-distribution failures
    // that leave funds stuck in the contract.
    if recipients.len() > 20 {
        panic!("TooManyRecipients: maximum 20 recipients allowed");
    }

    // 2. Validate recipients: no zero-share, no duplicates; BPS sums to 10000
    let mut total_bps: u32 = 0;
    for i in 0..recipients.len() {
        let r = recipients.get(i).unwrap();
        if r.share_bps == 0 {
            panic!("recipient share_bps cannot be zero");
        }
        for j in (i + 1)..recipients.len() {
            if r.address == recipients.get(j).unwrap().address {
                panic!("duplicate recipient address");
            }
        }
        total_bps += r.share_bps;
    }
    if total_bps != 10000 {
        panic!("InvalidShares: recipient shares must sum to exactly 10000 bps");
    }

    // 2. Increment and get Split ID
    let count = increment_counter(e, &DataKey::SplitCount);

    // 3. Move funds from sender to contract
    spend_balance(e, sender.clone(), total_amount);
    receive_balance(e, e.current_contract_address(), total_amount);

    // 4. Store record
    let record = SplitRecord {
        id: count,
        sender,
        recipients,
        total_amount,
        distributed: false,
        cancelled: false,
        memo: Bytes::new(e),
    };
    write_persistent_record(e, &DataKey::Split(count), &record);

    for recipient in record.recipients.iter() {
        append_recipient_split_index(e, &recipient.address, count);
    }

    count
}

pub fn distribute(e: &Env, caller: Address, split_id: u32) {
    distribute_internal(e, caller, split_id);
}

pub(crate) fn distribute_internal(e: &Env, caller: Address, split_id: u32) {

    let mut record: SplitRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Split(split_id))
        .expect("split record not found");
    e.storage().persistent().extend_ttl(
        &DataKey::Split(split_id),
        SPLIT_LIFETIME_THRESHOLD,
        SPLIT_BUMP_AMOUNT,
    );

    // 1. Rules: Caller must be sender, cannot distribute twice
    if record.sender != caller {
        panic!("unauthorized");
    }
    if record.distributed {
        panic!("already distributed");
    }
    if record.cancelled {
        panic!("split cancelled");
    }

    let sender_for_event = record.sender.clone();
    let amount_for_event = record.total_amount;

    let mut remaining_amount = record.total_amount;
    let len = record.recipients.len();

    // 3. Mark distributed BEFORE the loop — intentional re-entrancy protection.
    // Any re-entrant call to distribute sees distributed=true and panics immediately,
    // preventing double-distribution even if a recipient contract calls back.
    record.distributed = true;
    write_persistent_record(e, &DataKey::Split(split_id), &record);

    // 2. Proportional Distribution
    for (i, recipient) in record.recipients.iter().enumerate() {
        let amount_to_send = if i == (len as usize - 1) {
            // Last recipient gets everything left to avoid rounding dust
            remaining_amount
        } else {
            record
                .total_amount
                .checked_mul(recipient.share_bps as i128)
                .expect("split amount overflow")
                / 10000
        };

        // Transfer from contract to recipient
        spend_balance(e, e.current_contract_address(), amount_to_send);
        receive_balance(e, recipient.address.clone(), amount_to_send);

        remaining_amount = remaining_amount
            .checked_sub(amount_to_send)
            .expect("split remaining underflow");
    }

    // 4. Emit Observability Event
    e.events().publish(
        (
            symbol_short!("splt_dist"),
            split_id,
            sender_for_event,
        ),
        amount_for_event,
    );

    // 5. Update aggregate stats counters
    increment_counter(e, &DataKey::DistributedCount);
    let prev: i128 = e.storage().instance().get(&DataKey::TotalDistributedValue).unwrap_or(0);
    e.storage().instance().set(&DataKey::TotalDistributedValue, &(prev + amount_for_event));
}

pub fn cancel_split(e: &Env, caller: Address, split_id: u32) {

    let mut record: SplitRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Split(split_id))
        .expect("split record not found");
    e.storage().persistent().extend_ttl(
        &DataKey::Split(split_id),
        SPLIT_LIFETIME_THRESHOLD,
        SPLIT_BUMP_AMOUNT,
    );

    if record.sender != caller {
        panic!("unauthorized");
    }
    if record.distributed {
        panic!("already distributed");
    }
    if record.cancelled {
        panic!("already cancelled");
    }

    spend_balance(e, e.current_contract_address(), record.total_amount);
    receive_balance(e, caller.clone(), record.total_amount);

    record.cancelled = true;
    write_persistent_record(e, &DataKey::Split(split_id), &record);

    e.events().publish(
        (symbol_short!("splt_cxl"), split_id, caller),
        record.total_amount,
    );

    increment_counter(e, &DataKey::CancelledCount);
}

pub fn get_split(e: &Env, split_id: u32) -> SplitRecord {
    read_persistent_record(e, &DataKey::Split(split_id), "split record not found")
}

pub fn get_splits_for_recipient(e: &Env, recipient: Address) -> Vec<u32> {
    let key = DataKey::RecipientSplits(recipient);
    if let Some(ids) = e.storage().persistent().get::<DataKey, Vec<u32>>(&key) {
        e.storage().persistent().extend_ttl(&key, SPLIT_LIFETIME_THRESHOLD, SPLIT_BUMP_AMOUNT);
        ids
    } else {
        Vec::new(e)
    }
}

pub fn replace_split_recipient(
    e: &Env,
    sender: Address,
    split_id: u32,
    old_recipient: Address,
    new_recipient: Address,
) {
    let mut record: SplitRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Split(split_id))
        .expect("split not found");
    e.storage().persistent().extend_ttl(
        &DataKey::Split(split_id),
        SPLIT_LIFETIME_THRESHOLD,
        SPLIT_BUMP_AMOUNT,
    );
    if record.distributed {
        panic!("already distributed");
    }
    if record.cancelled {
        panic!("split cancelled");
    }
    if record.sender != sender {
        panic!("unauthorized");
    }
    if old_recipient == new_recipient {
        panic!("old and new recipient are the same");
    }

    let mut found = false;
    for i in 0..record.recipients.len() {
        let r = record.recipients.get(i).unwrap();
        if r.address == old_recipient {
            record.recipients.set(i, SplitRecipient {
                address: new_recipient.clone(),
                share_bps: r.share_bps,
            });
            found = true;
        } else if r.address == new_recipient {
            panic!("duplicate recipient address");
        }
    }
    if !found {
        panic!("old recipient not found in split");
    }

    write_persistent_record(e, &DataKey::Split(split_id), &record);
    remove_recipient_split_index(e, &old_recipient, split_id);
    append_recipient_split_index(e, &new_recipient, split_id);
    e.events().publish(
        (symbol_short!("splt_rplc"), split_id, sender),
        (old_recipient, new_recipient),
    );
}

/// Distributes multiple splits in a single invocation.
/// Caller must be the sender for every split; batch is rejected if any ID is
/// unauthorised. Maximum 10 split IDs per call.
pub fn bulk_distribute(e: &Env, caller: Address, split_ids: Vec<u32>) {
    if split_ids.len() > 10 {
        panic!("BulkLimit: maximum 10 split IDs per batch");
    }
    // Validate caller is sender for all splits before touching any funds
    for i in 0..split_ids.len() {
        let split_id = split_ids.get(i).unwrap();
        let record: SplitRecord = e
            .storage()
            .persistent()
            .get(&DataKey::Split(split_id))
            .expect("split not found");
        if record.sender != caller {
            panic!("unauthorized");
        }
    }
    // Execute
    for i in 0..split_ids.len() {
        let split_id = split_ids.get(i).unwrap();
        distribute_internal(e, caller.clone(), split_id);
    }
}

/// Creates a split and immediately locks each recipient's share in its own escrow.
/// Returns a `Vec<u32>` of escrow IDs in recipient order.
pub fn create_split_with_escrow(
    e: &Env,
    sender: Address,
    recipients: Vec<SplitRecipient>,
    total_amount: i128,
    expiry_ledger: u32,
) -> Vec<u32> {
    use crate::storage_types::{increment_counter, write_persistent_record};
    use crate::escrow::EscrowRecord;

    require_positive_amount(total_amount);
    if recipients.is_empty() {
        panic!("recipients cannot be empty");
    }
    spend_balance(e, sender.clone(), total_amount);
    receive_balance(e, e.current_contract_address(), total_amount);

    let mut escrow_ids = Vec::new(e);
    let len = recipients.len();
    let mut remaining = total_amount;

    for i in 0..len {
        let recipient = recipients.get(i).unwrap();
        let share = if i == len - 1 {
            remaining
        } else {
            total_amount
                .checked_mul(recipient.share_bps as i128)
                .expect("overflow")
                / 10000
        };
        remaining = remaining.checked_sub(share).expect("underflow");

        let escrow_id = increment_counter(e, &DataKey::EscrowCount);
        write_persistent_record(
            e,
            &DataKey::Escrow(escrow_id),
            &EscrowRecord {
                id: escrow_id,
                depositor: sender.clone(),
                beneficiary: recipient.address.clone(),
                amount: share,
                released: false,
                refunded: false,
                expiry_ledger,
            },
        );
        escrow_ids.push_back(escrow_id);
    }
    escrow_ids
}

/// Creates a split tagged with a memo (max 64 bytes) for off-chain correlation.
pub fn create_split_with_memo(
    e: &Env,
    sender: Address,
    recipients: Vec<SplitRecipient>,
    total_amount: i128,
    memo: Bytes,
) -> u32 {
    if memo.len() > 64 {
        panic!("MemoTooLong: memo cannot exceed 64 bytes");
    }
    let id = create_split(e, sender, recipients, total_amount);
    let mut record: SplitRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Split(id))
        .expect("split not found");
    record.memo = memo;
    use crate::storage_types::write_persistent_record;
    write_persistent_record(e, &DataKey::Split(id), &record);
    id
}
