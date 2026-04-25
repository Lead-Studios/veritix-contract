use crate::balance::{receive_balance, spend_balance};
use crate::storage_types::{
    increment_counter, read_persistent_record, write_persistent_record, DataKey,
};
use crate::validation::{require_current_or_future_ledger, require_positive_amount};
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRecord {
    pub id: u32,
    pub depositor: Address,
    pub beneficiary: Address,
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
    pub expiry_ledger: u32,
}

// Lock funds from depositor into escrow. The contract address itself holds the
// escrowed balance until the record is released or refunded. Returns the escrow ID.
pub fn create_escrow(
    e: &Env,
    depositor: Address,
    beneficiary: Address,
    amount: i128,
    expiry_ledger: u32,
) -> u32 {
    require_positive_amount(amount);
    require_current_or_future_ledger(e.ledger().sequence(), expiry_ledger);

    if depositor == beneficiary {
        panic!("InvalidEscrow: depositor and beneficiary cannot be the same address");
    }

    // Auth: depositor must authorize locking funds
    depositor.require_auth();

    // Move funds from depositor into the contract's balance
    spend_balance(e, depositor.clone(), amount);
    receive_balance(e, e.current_contract_address(), amount);

    // Increment and persist the global escrow counter
    let count = increment_counter(e, &DataKey::EscrowCount);

    // Persist the new escrow record
    let record = EscrowRecord {
        id: count,
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        amount,
        released: false,
        refunded: false,
        expiry_ledger,
    };
    write_persistent_record(e, &DataKey::Escrow(count), &record);

    // Append escrow ID to the depositor's list
    let key = DataKey::DepositorEscrows(depositor.clone());
    let mut ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(e));
    ids.push_back(count);
    e.storage().persistent().set(&key, &ids);

    // Optional observability event
    e.events().publish(
        (
            symbol_short!("escrow_created"),
            depositor.clone(),
            beneficiary.clone(),
        ),
        amount,
    );

    count
}

// Beneficiary claims the escrowed funds
pub fn release_escrow(e: &Env, caller: Address, escrow_id: u32) {
    try_release_escrow(e, caller, escrow_id).unwrap_or_else(|err| panic!("{}", err));
}

pub fn try_release_escrow(e: &Env, caller: Address, escrow_id: u32) -> Result<(), &'static str> {
    // Auth: caller must sign the transaction
    caller.require_auth();

    let mut escrow = try_get_escrow(e, escrow_id)?;

    // Authorization: only the beneficiary can release
    if escrow.beneficiary != caller {
        return Err("not beneficiary");
    }

    // State: cannot release twice or after refund
    if escrow.released || escrow.refunded {
        return Err("already settled");
    }

    // Mark as released and persist
    escrow.released = true;
    write_persistent_record(e, &DataKey::Escrow(escrow_id), &escrow);

    // Transfer funds from contract to beneficiary
    spend_balance(e, e.current_contract_address(), escrow.amount);
    receive_balance(e, escrow.beneficiary.clone(), escrow.amount);

    // Event for observability
    e.events().publish(
        (
            symbol_short!("escrow_released"),
            escrow_id,
            escrow.beneficiary.clone(),
        ),
        escrow.amount,
    );

    Ok(())
}

// Depositor reclaims funds — only if not yet released
pub fn refund_escrow(e: &Env, caller: Address, escrow_id: u32) {
    try_refund_escrow(e, caller, escrow_id).unwrap_or_else(|err| panic!("{}", err));
}

pub fn try_refund_escrow(e: &Env, caller: Address, escrow_id: u32) -> Result<(), &'static str> {
    // Auth: caller must sign the transaction
    caller.require_auth();

    let mut escrow = try_get_escrow(e, escrow_id)?;

    // Authorization: only the original depositor can refund, unless the escrow has expired
    let expired = e.ledger().sequence() > escrow.expiry_ledger;
    if escrow.depositor != caller && !expired {
        return Err("not depositor");
    }

    // State: cannot refund twice or after release
    if escrow.released || escrow.refunded {
        return Err("already settled");
    }

    // Mark as refunded and persist
    escrow.refunded = true;
    write_persistent_record(e, &DataKey::Escrow(escrow_id), &escrow);

    // Transfer funds from contract back to depositor
    spend_balance(e, e.current_contract_address(), escrow.amount);
    receive_balance(e, escrow.depositor.clone(), escrow.amount);

    // Event for observability
    e.events().publish(
        (
            symbol_short!("escrow_refunded"),
            escrow_id,
            escrow.depositor.clone(),
        ),
        escrow.amount,
    );

    Ok(())
}

// Read an escrow record by ID
pub fn get_escrow(e: &Env, escrow_id: u32) -> EscrowRecord {
    try_get_escrow(e, escrow_id).unwrap_or_else(|err| panic!("{}", err))
}

pub fn try_get_escrow(e: &Env, escrow_id: u32) -> Result<EscrowRecord, &'static str> {
    if e.storage().persistent().has(&DataKey::Escrow(escrow_id)) {
        Ok(read_persistent_record(
            e,
            &DataKey::Escrow(escrow_id),
            "escrow not found",
        ))
    } else {
        Err("escrow not found")
    }
}

// Returns all escrow IDs created by a given depositor.
pub fn get_escrows_by_depositor(e: &Env, depositor: Address) -> Vec<u32> {
    e.storage()
        .persistent()
        .get(&DataKey::DepositorEscrows(depositor))
        .unwrap_or_else(|| Vec::new(e))
}
