//! Escrow lifecycle module.
//! Manages escrow create/release/refund/admin-settle state transitions.
//! Contract balance represents escrowed funds held in custody until settlement.

use crate::admin::check_admin;
use crate::balance::{receive_balance, spend_balance};
use crate::storage_types::{
    increment_counter, read_persistent_record, write_persistent_record, DataKey,
    ESCROW_BUMP_AMOUNT, ESCROW_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD, WARNING_WINDOW,
};
use crate::validation::{require_current_or_future_ledger, require_positive_amount};
use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, Vec};

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

    // Optional observability event
    e.events().publish(
        (
            symbol_short!("escr_crtd"),
            depositor.clone(),
            beneficiary.clone(),
        ),
        amount,
    );

    // Maintain depositor index so escrow_between can scan by depositor
    let index_key = DataKey::DepositorEscrows(depositor.clone());
    let mut ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&index_key)
        .unwrap_or_else(|| vec![e]);
    ids.push_back(count);
    e.storage().persistent().set(&index_key, &ids);
    e.storage().persistent().extend_ttl(
        &index_key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    count
}

// Beneficiary claims the escrowed funds
pub fn release_escrow(e: &Env, caller: Address, escrow_id: u32) {
    try_release_escrow(e, caller, escrow_id).unwrap_or_else(|err| panic!("{}", err));
}

pub fn try_release_escrow(e: &Env, caller: Address, escrow_id: u32) -> Result<(), &'static str> {
    // Auth: caller must sign the transaction before reading escrow state.
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
            symbol_short!("escr_rls"),
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
    let mut escrow = try_get_escrow(e, escrow_id)?;

    // Block refund if an active dispute exists — the resolver must settle first.
    if e.storage().persistent().has(&DataKey::EscrowDispute(escrow_id)) {
        panic!("DisputeOpen: cannot refund while an active dispute is pending resolution");
    }

    // Authorization: only the original depositor can refund, unless the escrow has expired
    let expired = e.ledger().sequence() > escrow.expiry_ledger;
    if escrow.depositor != caller && !expired {
        return Err("not depositor");
    }

    // State: cannot refund twice or after release
    if escrow.released || escrow.refunded {
        return Err("already settled");
    }

    // Auth: caller must sign the transaction (after state checks)
    caller.require_auth();

    // Mark as refunded and persist
    escrow.refunded = true;
    write_persistent_record(e, &DataKey::Escrow(escrow_id), &escrow);

    // Transfer funds from contract back to depositor
    spend_balance(e, e.current_contract_address(), escrow.amount);
    receive_balance(e, escrow.depositor.clone(), escrow.amount);

    // Event for observability
    e.events().publish(
        (
            symbol_short!("escr_rfnd"),
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
    let key = DataKey::Escrow(escrow_id);
    if e.storage().persistent().has(&key) {
        e.storage()
            .persistent()
            .extend_ttl(&key, ESCROW_LIFETIME_THRESHOLD, ESCROW_BUMP_AMOUNT);
        let record: EscrowRecord = read_persistent_record(
            e,
            &key,
            "escrow not found",
        );
        if !record.released && !record.refunded {
            let warned_key = DataKey::ExpiryWarned(escrow_id);
            if !e.storage().instance().has(&warned_key)
                && record.expiry_ledger >= e.ledger().sequence()
                && record.expiry_ledger - e.ledger().sequence() < WARNING_WINDOW
            {
                e.storage().instance().set(&warned_key, &true);
                e.events().publish(
                    (symbol_short!("exp_warn"), escrow_id),
                    (record.expiry_ledger, e.ledger().sequence()),
                );
            }
        }
        Ok(record)
    } else {
        Err("escrow not found")
    }
}

/// Returns aggregate stats across all escrows: total count, active count,
/// settled count, and total value currently locked in active escrows.
pub fn escrow_stats(e: &Env) -> crate::storage_types::EscrowStats {
    let total_count = crate::storage_types::read_counter(e, &DataKey::EscrowCount);
    let mut active_count: u32 = 0;
    let mut total_value_locked: i128 = 0;
    for id in 1..=total_count {
        if let Some(record) = e.storage().persistent().get::<DataKey, EscrowRecord>(&DataKey::Escrow(id)) {
            if !record.released && !record.refunded {
                active_count += 1;
                total_value_locked += record.amount;
            }
        }
    }
    crate::storage_types::EscrowStats {
        total_count,
        active_count,
        settled_count: total_count - active_count,
        total_value_locked,
    }
}
pub fn topup_escrow(e: &Env, depositor: Address, escrow_id: u32, amount: i128) {
    depositor.require_auth();
    require_positive_amount(amount);
    if e.storage().persistent().has(&DataKey::EscrowDispute(escrow_id)) {
        panic!("DisputeOpen: cannot top up an escrow under active dispute");
    }
    let mut record = get_escrow(e, escrow_id);
    if record.released || record.refunded {
        panic!("escrow already settled");
    }
    if record.depositor != depositor {
        panic!("not the depositor");
    }
    spend_balance(e, depositor.clone(), amount);
    receive_balance(e, e.current_contract_address(), amount);
    record.amount += amount;
    write_persistent_record(e, &DataKey::Escrow(escrow_id), &record);
}

/// Admin escape hatch: forcibly settles a stuck escrow by sending funds to
/// `recipient`. Used when the normal beneficiary or depositor is frozen and
/// the standard release/refund paths are deadlocked.
///
/// Only the contract admin may call this. The escrow must not already be settled.
pub fn admin_settle_escrow(e: &Env, admin: Address, escrow_id: u32, recipient: Address) {
    check_admin(e, &admin);

    let mut escrow = try_get_escrow(e, escrow_id)
        .unwrap_or_else(|err| panic!("{}", err));

    if escrow.released || escrow.refunded {
        panic!("already settled");
    }

    escrow.released = true;
    write_persistent_record(e, &DataKey::Escrow(escrow_id), &escrow);

    spend_balance(e, e.current_contract_address(), escrow.amount);
    receive_balance(e, recipient.clone(), escrow.amount);

    e.events().publish(
        (symbol_short!("adm_sttl"), escrow_id, admin),
        (recipient, escrow.amount),
    );
}

pub fn escrowed_value_for_depositor(e: &Env, depositor: Address) -> i128 {
    let key = DataKey::DepositorEscrows(depositor);
    let ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| vec![e]);
    e.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    let mut total = 0i128;
    for id in ids.iter() {
        if let Ok(record) = try_get_escrow(e, id) {
            if !record.released && !record.refunded {
                total += record.amount;
            }
        }
    }
    total
}

/// Returns the first active escrow ID between depositor and beneficiary, or None.
pub fn escrow_between(e: &Env, depositor: Address, beneficiary: Address) -> Option<u32> {
    let key = DataKey::DepositorEscrows(depositor);
    let ids: Vec<u32> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| vec![e]);
    for id in ids.iter() {
        if let Ok(record) = try_get_escrow(e, id) {
            if record.beneficiary == beneficiary && !record.released && !record.refunded {
                return Some(id);
            }
        }
    }
    None
}
