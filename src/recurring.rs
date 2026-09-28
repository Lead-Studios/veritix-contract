//! Execution of authorised recurring charges and retained execution history.

use soroban_sdk::{Env, Vec};

use crate::balance;
use crate::storage_types::{DataKey, RecurringExecution, RecurringPayment};
use crate::validation::require_positive_amount;

/// Maximum number of successful charges retained for each schedule.
pub const MAX_RECURRING_HISTORY: u32 = 100;

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

/// Records a successful charge, keeping only the newest entries.
///
/// Call this only after the token transfer has succeeded.
pub fn record_execution(e: &Env, recurring_id: u64, amount: i128) {
    let key = DataKey::RecurringHistory(recurring_id);
    let mut history = e
        .storage()
        .persistent()
        .get::<_, Vec<RecurringExecution>>(&key)
        .unwrap_or_else(|| Vec::new(e));

    while history.len() >= MAX_RECURRING_HISTORY {
        history.remove(0);
    }
    history.push_back(RecurringExecution {
        ledger: e.ledger().sequence(),
        amount,
    });
    e.storage().persistent().set(&key, &history);
}

/// Returns the retained successful charge history for `recurring_id`.
///
/// Returns an empty vector if no charge has been recorded for the schedule.
pub fn get_recurring_history(e: &Env, recurring_id: u64) -> Vec<RecurringExecution> {
    e.storage()
        .persistent()
        .get(&DataKey::RecurringHistory(recurring_id))
        .unwrap_or_else(|| Vec::new(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::balance_of;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};
    use soroban_sdk::{contract, contractimpl, Address, Env};

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

    #[contract]
    pub struct HistoryHarness;

    #[contractimpl]
    impl HistoryHarness {
        pub fn record(e: Env, recurring_id: u64, amount: i128) {
            record_execution(&e, recurring_id, amount);
        }

        pub fn history(e: Env, recurring_id: u64) -> Vec<RecurringExecution> {
            get_recurring_history(&e, recurring_id)
        }
    }

    #[test]
    fn history_retains_only_the_newest_executions() {
        let e = Env::default();
        let contract_id = e.register_contract(None, HistoryHarness);
        let client = HistoryHarnessClient::new(&e, &contract_id);

        assert_eq!(client.history(&9).len(), 0);
        for amount in 0..=MAX_RECURRING_HISTORY {
            client.record(&9, &(amount as i128));
        }

        let history = client.history(&9);
        assert_eq!(history.len(), MAX_RECURRING_HISTORY);
        assert_eq!(history.get(0).unwrap().amount, 1);
        assert_eq!(history.get(MAX_RECURRING_HISTORY - 1).unwrap().amount, 100);
    }
}