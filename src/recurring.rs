//! Execution of authorised recurring charges.

use soroban_sdk::Env;

use crate::balance;
use crate::storage_types::{DataKey, RecurringPayment};
use crate::validation::require_positive_amount;

/// Pulls one due interval from the payer and schedules the next interval.
///
/// # Panics
///
/// With `RecurringNotFound`, `RecurringNotDue`, `RecurringInactive`,
/// `RecurringPaused`, `RecurringIntervalInvalid`, or `RecurringLedgerOverflow`
/// when the schedule cannot be executed. The balance transfer also panics with
/// `InsufficientBalance` when the payer cannot cover the charge.
pub fn execute_recurring(e: &Env, recurring_id: u64) {
    let key = DataKey::Recurring(recurring_id);
    let mut schedule = e
        .storage()
        .persistent()
        .get::<_, RecurringPayment>(&key)
        .unwrap_or_else(|| panic!("RecurringNotFound: payment {} does not exist", recurring_id));

    if !schedule.active {
        panic!("RecurringInactive: payment {} is inactive", recurring_id);
    }
    if schedule.paused {
        panic!("RecurringPaused: payment {} is paused", recurring_id);
    }
    if e.ledger().sequence() < schedule.next_execution {
        panic!(
            "RecurringNotDue: payment {} is due at ledger {}",
            recurring_id, schedule.next_execution
        );
    }
    if schedule.interval_ledgers == 0 {
        panic!("RecurringIntervalInvalid: interval must be positive");
    }

    let next_execution = schedule
        .next_execution
        .checked_add(schedule.interval_ledgers)
        .unwrap_or_else(|| panic!("RecurringLedgerOverflow: next execution ledger overflows"));
    require_positive_amount(schedule.amount);
    balance::transfer(e, &schedule.payer, &schedule.payee, schedule.amount);

    schedule.next_execution = next_execution;
    e.storage().persistent().set(&key, &schedule);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::balance_of;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env};

    fn setup(
        next_execution: u32,
        active: bool,
        paused: bool,
        payer_balance: i128,
    ) -> (Env, Address, Address, Address) {
        let e = Env::default();
        let contract_id = Address::generate(&e);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);
        e.ledger().set(LedgerInfo {
            protocol_version: 28,
            sequence_number: 100,
            ..Default::default()
        });
        e.as_contract(&contract_id, || {
            e.storage().persistent().set(
                &DataKey::Recurring(1),
                &RecurringPayment {
                    payer: payer.clone(),
                    payee: payee.clone(),
                    token: Address::generate(&e),
                    amount: 25,
                    interval_ledgers: 10,
                    next_execution,
                    active,
                    paused,
                },
            );
            balance::credit(&e, &payer, payer_balance);
        });
        (e, contract_id, payer, payee)
    }

    #[test]
    fn execution_transfers_one_charge_and_advances_one_interval() {
        let (e, contract_id, payer, payee) = setup(100, true, false, 100);

        e.as_contract(&contract_id, || execute_recurring(&e, 1));

        e.as_contract(&contract_id, || {
            let schedule: RecurringPayment = e
                .storage()
                .persistent()
                .get(&DataKey::Recurring(1))
                .unwrap();
            assert_eq!(schedule.next_execution, 110);
            assert_eq!(balance_of(&e, &payer), 75);
            assert_eq!(balance_of(&e, &payee), 25);
        });
    }

    #[test]
    #[should_panic(expected = "RecurringNotDue")]
    fn execution_before_due_ledger_is_rejected() {
        let (e, contract_id, _, _) = setup(101, true, false, 100);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[test]
    #[should_panic(expected = "RecurringInactive")]
    fn inactive_schedule_is_rejected() {
        let (e, contract_id, _, _) = setup(100, false, false, 100);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[test]
    #[should_panic(expected = "RecurringPaused")]
    fn paused_schedule_is_rejected() {
        let (e, contract_id, _, _) = setup(100, true, true, 100);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }

    #[test]
    #[should_panic(expected = "InsufficientBalance")]
    fn insufficient_payer_balance_is_rejected() {
        let (e, contract_id, _, _) = setup(100, true, false, 24);
        e.as_contract(&contract_id, || execute_recurring(&e, 1));
    }
}