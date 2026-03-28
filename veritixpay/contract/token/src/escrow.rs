use crate::balance::{receive_balance, spend_balance};
use crate::storage_types::{read_persistent_record, write_persistent_record, DataKey};
use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRecord {
    pub id: u32,
    pub depositor: Address,
    pub beneficiary: Address,
    pub amount: i128,
    pub released: bool,
    pub refunded: bool,
}

// Lock funds from depositor into escrow. Returns the escrow ID.
pub fn create_escrow(e: &Env, depositor: Address, beneficiary: Address, amount: i128) -> u32 {
    // Auth: depositor must authorize locking funds
    depositor.require_auth();

    // Move funds from depositor into the contract's balance
    spend_balance(e, depositor.clone(), amount);
    receive_balance(e, e.current_contract_address(), amount);

    // Increment and persist the global escrow counter
    let mut count: u32 = e
        .storage()
        .instance()
        .get(&DataKey::EscrowCount)
        .unwrap_or(0);
    count += 1;
    e.storage().instance().set(&DataKey::EscrowCount, &count);

    // Persist the new escrow record
    let record = EscrowRecord {
        id: count,
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        amount,
        released: false,
        refunded: false,
    };
    write_persistent_record(e, &DataKey::Escrow(count), &record);

    // Optional observability event
    e.events().publish(
        (
            Symbol::new(e, "escrow"),
            Symbol::new(e, "created"),
            depositor,
        ),
        (beneficiary, amount),
    );

    count
}

// Beneficiary claims the escrowed funds
pub fn release_escrow(e: &Env, caller: Address, escrow_id: u32) {
    // Auth: caller must sign the transaction
    caller.require_auth();

    let mut escrow = get_escrow(e, escrow_id);

    // Authorization: only the beneficiary can release
    if escrow.beneficiary != caller {
        panic!("not beneficiary");
    }

    // State: cannot release twice or after refund
    if escrow.released || escrow.refunded {
        panic!("already settled");
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
            Symbol::new(e, "escrow"),
            Symbol::new(e, "released"),
            escrow_id,
        ),
        escrow.beneficiary,
    );
}

// Depositor reclaims funds — only if not yet released
pub fn refund_escrow(e: &Env, caller: Address, escrow_id: u32) {
    // Auth: caller must sign the transaction
    caller.require_auth();

    let mut escrow = get_escrow(e, escrow_id);

    // Authorization: only the original depositor can refund
    if escrow.depositor != caller {
        panic!("not depositor");
    }

    // State: cannot refund twice or after release
    if escrow.released || escrow.refunded {
        panic!("already settled");
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
            Symbol::new(e, "escrow"),
            Symbol::new(e, "refunded"),
            escrow_id,
        ),
        escrow.depositor,
    );
}

// Read an escrow record by ID
pub fn get_escrow(e: &Env, escrow_id: u32) -> EscrowRecord {
    read_persistent_record(e, &DataKey::Escrow(escrow_id), "escrow not found")
}
