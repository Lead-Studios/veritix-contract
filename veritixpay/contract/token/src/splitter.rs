use crate::balance::{receive_balance, spend_balance};
use crate::storage_types::{read_persistent_record, write_persistent_record, DataKey};
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

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
}

pub fn create_split(
    e: &Env,
    sender: Address,
    recipients: Vec<SplitRecipient>,
    total_amount: i128,
) -> u32 {
    sender.require_auth();

    // 1. Validate BPS Sums to 10000 (100.00%)
    let mut total_bps: u32 = 0;
    for recipient in recipients.iter() {
        total_bps += recipient.share_bps;
    }
    if total_bps != 10000 {
        panic!("total bps must equal 10000");
    }

    // 2. Increment and get Split ID
    let mut count: u32 = e.storage().instance().get(&DataKey::SplitCount).unwrap_or(0);
    count += 1;
    e.storage().instance().set(&DataKey::SplitCount, &count);

    // 3. Move funds from sender to contract
    // Note: Assuming contract address is e.current_contract_address()
    spend_balance(e, sender.clone(), total_amount);
    receive_balance(e, e.current_contract_address(), total_amount);

    // 4. Store record
    let record = SplitRecord {
        id: count,
        sender,
        recipients,
        total_amount,
        distributed: false,
    };
    write_persistent_record(e, &DataKey::Split(count), &record);

    count
}

pub fn distribute(e: &Env, caller: Address, split_id: u32) {
    caller.require_auth();

    let mut record: SplitRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Split(split_id))
        .expect("split record not found");

    // 1. Rules: Caller must be sender, cannot distribute twice
    if record.sender != caller {
        panic!("unauthorized");
    }
    if record.distributed {
        panic!("already distributed");
    }

    let mut remaining_amount = record.total_amount;
    let len = record.recipients.len();

    // 2. Proportional Distribution
    for (i, recipient) in record.recipients.iter().enumerate() {
        let amount_to_send = if i == (len as usize - 1) {
            // Last recipient gets everything left to avoid rounding dust
            remaining_amount
        } else {
            (record.total_amount * recipient.share_bps as i128) / 10000
        };

        // Transfer from contract to recipient
        spend_balance(e, e.current_contract_address(), amount_to_send);
        receive_balance(e, recipient.address.clone(), amount_to_send);
        
        remaining_amount -= amount_to_send;
    }

    // 3. Mark distributed
    record.distributed = true;
    write_persistent_record(e, &DataKey::Split(split_id), &record);

    // 4. Emit Observability Event
    e.events().publish(
        (Symbol::new(e, "split"), Symbol::new(e, "distributed"), split_id),
        record.total_amount
    );
}

pub fn get_split(e: &Env, split_id: u32) -> SplitRecord {
    read_persistent_record(e, &DataKey::Split(split_id), "split record not found")
}
