use crate::balance::{receive_balance, spend_balance};
use crate::storage_types::{increment_counter, DataKey};
use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringRecord {
    pub id: u32,
    pub payer: Address,
    pub payee: Address,
    pub amount: i128,
    pub interval: u32,
    pub last_charged_ledger: u32,
    pub active: bool,
}

/// Sets up a new recurring payment configuration.
pub fn setup_recurring(
    e: &Env,
    payer: Address,
    payee: Address,
    amount: i128,
    interval: u32,
) -> u32 {
    // 1. Authorization: The payer must explicitly authorize this recurring charge
    payer.require_auth();

    // 2. Increment and get the new Recurring ID
    let count = increment_counter(e, &DataKey::RecurringCount);

    // 3. Store the recurring record
    let record = RecurringRecord {
        id: count,
        payer: payer.clone(),
        payee: payee.clone(),
        amount,
        interval,
        last_charged_ledger: e.ledger().sequence(), // Set initial timestamp to now
        active: true,
    };
    e.storage().persistent().set(&DataKey::Recurring(count), &record);

    // 4. Emit Observability Event
    e.events().publish(
        (Symbol::new(e, "recurring"), Symbol::new(e, "setup"), payer),
        (payee, amount)
    );

    count
}

/// Executes a recurring payment if the interval has passed. 
/// Anyone can call this ("crank the contract"), but funds only move from payer to payee.
pub fn execute_recurring(e: &Env, recurring_id: u
