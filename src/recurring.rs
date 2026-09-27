//! Retained execution history for recurring schedules.

use soroban_sdk::{Env, Vec};

use crate::storage_types::{DataKey, RecurringExecution};

/// Maximum number of successful charges retained for each schedule.
pub const MAX_RECURRING_HISTORY: u32 = 100;

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
    use soroban_sdk::{contract, contractimpl};

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