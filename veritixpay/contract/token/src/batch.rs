use crate::admin::check_admin;
use crate::allowance::write_allowance;
use crate::balance::{decrease_supply, increase_supply, receive_balance, spend_balance};
use crate::freeze::{freeze_account, unfreeze_account};
use crate::validation::require_positive_amount;
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env, Vec};

const MAX_BATCH_TARGETS: u32 = 50;

#[contracttype]
#[derive(Clone)]
pub struct BatchEntry {
    pub address: Address,
    pub amount: i128,
}

pub fn clawback_batch(e: &Env, admin: Address, targets: Vec<(Address, i128)>) {
    check_admin(e, &admin);
    if targets.len() > MAX_BATCH_TARGETS {
        panic!("batch too large");
    }
    for i in 0..targets.len() {
        let (from, amount) = targets.get(i).unwrap();
        if from == e.current_contract_address() {
            panic!("InvalidClawback: cannot clawback from the contract address");
        }
        require_positive_amount(amount);
        spend_balance(e, from.clone(), amount);
        decrease_supply(e, amount);
        e.events().publish((symbol_short!("clawback"), admin.clone(), from), amount);
    }
}

pub fn freeze_batch(e: &Env, admin: Address, targets: Vec<Address>) {
    check_admin(e, &admin);
    if targets.len() > MAX_BATCH_TARGETS {
        panic!("batch too large");
    }
    for i in 0..targets.len() {
        let target = targets.get(i).unwrap();
        freeze_account(e, admin.clone(), target);
    }
    e.events().publish((symbol_short!("batch_frz"), admin), targets.len());
}

pub fn unfreeze_batch(e: &Env, admin: Address, targets: Vec<Address>) {
    check_admin(e, &admin);
    if targets.len() > MAX_BATCH_TARGETS {
        panic!("batch too large");
    }
    for i in 0..targets.len() {
        let target = targets.get(i).unwrap();
        unfreeze_account(e, admin.clone(), target);
    }
    e.events().publish((symbol_short!("batch_unf"), admin), targets.len());
}

pub fn mint_batch(e: &Env, admin: Address, recipients: Vec<BatchEntry>) {
    check_admin(e, &admin);
    if recipients.len() > MAX_BATCH_TARGETS {
        panic!("BatchLimit: maximum 50 recipients per call");
    }
    let mut total: i128 = 0;
    for i in 0..recipients.len() {
        let entry = recipients.get(i).unwrap();
        require_positive_amount(entry.amount);
        receive_balance(e, entry.address.clone(), entry.amount);
        increase_supply(e, entry.amount);
        total = total.checked_add(entry.amount).expect("overflow");
    }
    e.events().publish((symbol_short!("btch_mint"), admin), total);
}

pub fn burn_from_batch(e: &Env, spender: Address, targets: Vec<(Address, i128)>) {
    spender.require_auth();
    if targets.len() > MAX_BATCH_TARGETS {
        panic!("batch too large");
    }
    for i in 0..targets.len() {
        let (from, amount) = targets.get(i).unwrap();
        require_positive_amount(amount);
        crate::allowance::spend_allowance(e, from.clone(), spender.clone(), amount);
        spend_balance(e, from.clone(), amount);
        decrease_supply(e, amount);
        e.events().publish((symbol_short!("burn_from"), spender.clone(), from), amount);
    }
}

pub fn transfer_batch(e: &Env, from: Address, recipients: Vec<BatchEntry>) {
    from.require_auth();
    if recipients.len() > MAX_BATCH_TARGETS {
        panic!("BatchLimit: maximum 50 recipients per call");
    }
    let mut total: i128 = 0;
    for i in 0..recipients.len() {
        let entry = recipients.get(i).unwrap();
        require_positive_amount(entry.amount);
        total = total.checked_add(entry.amount).expect("overflow");
    }
    spend_balance(e, from.clone(), total);
    for i in 0..recipients.len() {
        let entry = recipients.get(i).unwrap();
        receive_balance(e, entry.address.clone(), entry.amount);
    }
    e.events().publish((symbol_short!("btch_xfer"), from), total);
}

const MAX_APPROVE_BATCH: u32 = 20;

pub fn approve_batch(e: &Env, from: Address, approvals: Vec<(Address, i128, u32)>) {
    from.require_auth();
    if approvals.len() > MAX_APPROVE_BATCH {
        panic!("batch too large");
    }
    for i in 0..approvals.len() {
        let (spender, amount, expiration_ledger) = approvals.get(i).unwrap();
        write_allowance(e, from.clone(), spender.clone(), amount, expiration_ledger);
        e.events().publish((symbol_short!("approve"), from.clone(), spender), amount);
    }
}

pub fn transfer_batch_with_memo(e: &Env, from: Address, recipients: Vec<(Address, i128, Bytes)>) {
    from.require_auth();
    if recipients.len() > MAX_BATCH_TARGETS {
        panic!("batch too large");
    }
    let mut total: i128 = 0;
    for i in 0..recipients.len() {
        let (_, amount, memo) = recipients.get(i).unwrap();
        require_positive_amount(amount);
        if memo.len() > 64 {
            panic!("memo too long");
        }
        total = total.checked_add(amount).expect("overflow");
    }
    spend_balance(e, from.clone(), total);
    for i in 0..recipients.len() {
        let (to, amount, memo) = recipients.get(i).unwrap();
        receive_balance(e, to.clone(), amount);
        e.events().publish((symbol_short!("xfer_memo"), from.clone(), to), (amount, memo));
    }
}
