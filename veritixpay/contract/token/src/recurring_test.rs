#[cfg(test)]
mod recurring_tests {
    use soroban_sdk::{
        testutils::{Address as _, Events as _, Ledger as _},
        Address, Env, Symbol, TryFromVal,
    };

    use crate::balance::read_balance;
    use crate::contract::{VeritixToken, VeritixTokenClient};
    use crate::recurring::{
        cancel_recurring, execute_recurring, get_recurring,
        get_recurring_by_payer, setup_recurring,
        transfer_recurring_payer,
    };
    use crate::storage_types::{read_counter, DataKey};

    fn setup_env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    fn fund_and_setup(
        e: &Env,
        contract_id: &Address,
        amount: i128,
        interval: u32,
    ) -> (Address, Address, u32) {
        let payer = Address::generate(e);
        let payee = Address::generate(e);
        let mut id = 0u32;
        e.as_contract(contract_id, || {
            crate::balance::receive_balance(e, payer.clone(), amount);
            id = setup_recurring(e, payer.clone(), payee.clone(), amount, interval);
        });
        (payer, payee, id)
    }

    // Ensures that creating a recurring payment where payer == payee is
    // rejected — prevents degenerate self-payment schedules.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidRecurring: payer and payee cannot be the same address")]
    fn test_setup_recurring_same_address_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let addr = Address::generate(&e);

        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, addr.clone(), 500);
            setup_recurring(&e, addr.clone(), addr.clone(), 500, 100);
        });
    }

    // Verifies that setup_recurring stores a record with correct payer, payee,
    // amount, interval, and active state. If this fails, recurring payment
    // setup is broken.
    #[test]
    fn test_setup_stores_record() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            let r = get_recurring(&e, id);
            assert_eq!(r.payer, payer);
            assert_eq!(r.payee, payee);
            assert_eq!(r.amount, 500);
            assert_eq!(r.interval, 100);
            assert!(r.active);
        });
    }

    // Happy-path execute: advances ledger past the interval and verifies that
    // funds move from payer to payee correctly.
    #[test]
    fn test_execute_transfers_funds() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            // Advance ledger past the interval.
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 500);
            assert_eq!(read_balance(&e, payer.clone()), 0);
        });
    }

    // Ensures that executing a recurring payment before the interval has
    // elapsed panics — prevents early withdrawal of funds.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "interval has not elapsed")]
    fn test_execute_too_early_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            // Only advance by 50 — not enough.
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 50);
            execute_recurring(&e, id);
        });
    }

    // Verifies that cancelling a recurring payment deactivates the record
    // and prevents future executions.
    #[test]
    fn test_cancel_deactivates_record() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            cancel_recurring(&e, payer.clone(), id);
            let r = get_recurring(&e, id);
            assert!(!r.active);
        });
    }

    // Ensures that only the payer can cancel a recurring payment — a third
    // party hacker must be rejected with "unauthorized".
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "unauthorized")]
    fn test_cancel_unauthorized_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        let hacker = Address::generate(&e);

        e.as_contract(&contract_id, || {
            cancel_recurring(&e, hacker, id);
        });
    }

    // Ensures that executing a cancelled recurring payment panics — prevents
    // funds from being transferred after the payer has deactivated the plan.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "recurring payment is not active")]
    fn test_execute_after_cancel_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            cancel_recurring(&e, payer.clone(), id);
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 200);
            execute_recurring(&e, id);
        });
    }

    // Ensures that creating a recurring payment with zero amount is rejected.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "amount must be positive")]
    fn test_recurring_zero_amount_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);
        e.as_contract(&contract_id, || {
            setup_recurring(&e, payer.clone(), payee.clone(), 0, 100);
        });
    }

    // Ensures that creating a recurring payment with zero interval is rejected.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "interval must be positive")]
    fn test_recurring_zero_interval_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), 500);
            setup_recurring(&e, payer.clone(), payee.clone(), 500, 0);
        });
    }

    // Verifies that setup_recurring requires the payee's authorization in
    // addition to the payer's — both parties must consent to the schedule.
    #[test]
    fn test_recurring_payee_auth_required() {
        let e = Env::default();
        let contract_id = e.register_contract(None, VeritixToken);
        let client = VeritixTokenClient::new(&e, &contract_id);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);

        e.mock_all_auths();
        client.setup_recurring(&payer, &payee, &500i128, &100u32);

        // Payee must be among the authorized signers
        let auths = e.auths();
        let payee_authorized = auths.iter().any(|(addr, _)| addr == &payee);
        assert!(payee_authorized, "payee must authorize setup_recurring");
    }

    // Ensures that execute_recurring panics when the payer has insufficient
    // balance to cover the recurring amount.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InsufficientBalance")]
    fn test_execute_recurring_insufficient_balance_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        // Fund payer with less than the recurring amount.
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        e.as_contract(&contract_id, || {
            // Drain the payer balance so they can no longer cover the charge.
            crate::balance::spend_balance(&e, payer.clone(), 500);
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);
        });
    }

    // Verifies that a recurring payment can be executed multiple times as long
    // as sufficient balance remains and intervals elapse between executions.
    #[test]
    fn test_multiple_executions() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        // Fund payer with enough for two charges.
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 1_000, 100);
        // Give payer extra balance for second charge.
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), 1_000);
        });

        e.as_contract(&contract_id, || {
            let start = e.ledger().sequence();

            e.ledger().with_mut(|l| l.sequence_number = start + 101);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 1_000);

            e.ledger().with_mut(|l| l.sequence_number = start + 202);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 2_000);
        });
    }

    // Verifies that both payer and payee appear in the auths for
    // setup_recurring, confirming the dual-authorization requirement.
    #[test]
    fn test_setup_recurring_requires_both_payer_and_payee_auth() {
        let e = Env::default();
        let contract_id = e.register_contract(None, VeritixToken);
        let client = VeritixTokenClient::new(&e, &contract_id);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);
        let amount = 500i128;
        let interval = 100u32;

        e.mock_all_auths();
        client.setup_recurring(&payer, &payee, &amount, &interval);

        // Both payer and payee must have authorized the setup_recurring call.
        let auths = e.auths();
        assert_eq!(auths.len(), 2, "expected both payer and payee auth");
        assert_eq!(auths[0].0, payer, "first auth should be payer");
        assert_eq!(auths[1].0, payee, "second auth should be payee");
    }

    // Ensures that executing a recurring payment preserves the supply invariant
    // (sum of balances == total supply) before and after execution.
    #[test]
    fn test_recurring_execute_preserves_supply() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 1_000, 100);

        e.as_contract(&contract_id, || {
            crate::balance::increase_supply(&e, 1_000);

            let assert_invariant = || {
                let sum = read_balance(&e, payer.clone()) + read_balance(&e, payee.clone());
                assert_eq!(crate::balance::read_total_supply(&e), sum);
            };

            assert_invariant();

            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);
            assert_invariant();

            // Fund payer for a second execution
            crate::balance::receive_balance(&e, payer.clone(), 1_000);
            crate::balance::increase_supply(&e, 1_000);
            assert_invariant();

            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);
            assert_invariant();
        });
    }

    // --- Issue #162: Event emission tests ---

    // Verifies that setup_recurring emits a single event with
    // (recurring_setup, payer) topics and (payee, amount) as data.
    #[test]
    fn test_setup_recurring_emits_event() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, _id) = fund_and_setup(&e, &contract_id, 500, 100);

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        // Topics: (recurring_setup, payer), data: (payee, amount)
        assert_eq!(events.first().unwrap().1.len(), 2);
    }

    // Verifies that execute_recurring emits a single event with
    // (recurring_executed, recurring_id) topics and amount as data.
    #[test]
    fn test_execute_recurring_emits_event() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        // Clear setup event
        let _ = e.events().all();

        e.as_contract(&contract_id, || {
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);
        });

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        // Topics: (recurring_executed, recurring_id), data: amount
        assert_eq!(events.first().unwrap().1.len(), 2);
    }

    // Verifies that cancel_recurring emits a single event with
    // (recurring_cancelled, recurring_id, caller) topics.
    #[test]
    fn test_cancel_recurring_emits_event() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        let _ = e.events().all();

        e.as_contract(&contract_id, || {
            cancel_recurring(&e, payer.clone(), id);
        });

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        // Topics: (recurring_cancelled, recurring_id, caller), data: ()
        assert_eq!(events.first().unwrap().1.len(), 3);
    }

    // --- Recurring counter tests ---

    // Ensures the recurring counter starts at zero before any payments are set up.
    #[test]
    fn test_recurring_count_starts_at_zero() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);

        e.as_contract(&contract_id, || {
            let count = read_counter(&e, &DataKey::RecurringCount);
            assert_eq!(count, 0);
        });
    }

    // Verifies the recurring counter increments correctly with IDs 1, 2, 3
    // across multiple setup calls — no ID gaps or collisions.
    #[test]
    fn test_recurring_count_increments_on_setup() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let payer1 = Address::generate(&e);
        let payee1 = Address::generate(&e);
        let payer2 = Address::generate(&e);
        let payee2 = Address::generate(&e);
        let payer3 = Address::generate(&e);
        let payee3 = Address::generate(&e);

        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer1.clone(), 1000);
            crate::balance::receive_balance(&e, payer2.clone(), 1000);
            crate::balance::receive_balance(&e, payer3.clone(), 1000);

            // Before any recurring payments
            assert_eq!(read_counter(&e, &DataKey::RecurringCount), 0);

            // Setup first recurring
            let id = setup_recurring(&e, payer1.clone(), payee1.clone(), 500, 100);
            assert_eq!(id, 1);
            assert_eq!(read_counter(&e, &DataKey::RecurringCount), 1);

            // Setup second recurring
            let id = setup_recurring(&e, payer2.clone(), payee2.clone(), 500, 100);
            assert_eq!(id, 2);
            assert_eq!(read_counter(&e, &DataKey::RecurringCount), 2);

            // Setup third recurring
            let id = setup_recurring(&e, payer3.clone(), payee3.clone(), 500, 100);
            assert_eq!(id, 3);
            assert_eq!(read_counter(&e, &DataKey::RecurringCount), 3);
        });
    }

    // Ensures that creating a recurring payment with zero interval is rejected
    // with "InvalidInterval" — matches the updated panic string convention.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidInterval")]
    fn test_setup_recurring_zero_interval_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);

        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), 500);
            setup_recurring(&e, payer.clone(), payee.clone(), 500, 0);
        });
    }

    // --- Issue #450: cancel_recurring_batch tests ---

    #[test]
    fn test_cancel_recurring_batch_deactivates_all() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id1) = fund_and_setup(&e, &contract_id, 500, 100);

        // Setup a second recurring for the same payer.
        let payee2 = Address::generate(&e);
        let mut id2 = 0u32;
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), 500);
            id2 = setup_recurring(&e, payer.clone(), payee2.clone(), 500, 100);
        });

        e.as_contract(&contract_id, || {
            let ids = soroban_sdk::vec![&e, id1, id2];
            crate::recurring::cancel_recurring_batch(&e, payer.clone(), ids);
            assert!(!get_recurring(&e, id1).active);
            assert!(!get_recurring(&e, id2).active);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "unauthorized")]
    fn test_cancel_recurring_batch_unauthorized_reverts_all() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id1) = fund_and_setup(&e, &contract_id, 500, 100);

        // Create a second recurring owned by a different payer.
        let other_payer = Address::generate(&e);
        let payee2 = Address::generate(&e);
        let mut id2 = 0u32;
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, other_payer.clone(), 500);
            id2 = setup_recurring(&e, other_payer.clone(), payee2.clone(), 500, 100);
        });

        // Attempt to cancel both — id2 belongs to other_payer, must panic (full revert).
        e.as_contract(&contract_id, || {
            let ids = soroban_sdk::vec![&e, id1, id2];
            crate::recurring::cancel_recurring_batch(&e, payer.clone(), ids);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "batch exceeds maximum of 20")]
    fn test_cancel_recurring_batch_over_limit_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let payer = Address::generate(&e);

        // Register a second VeritixToken contract as the payee
        let payee_contract_id = e.register_contract(None, VeritixToken);
        let payee_client = crate::contract::VeritixTokenClient::new(&e, &payee_contract_id);
        let payee_admin = Address::generate(&e);
        payee_client.initialize(
            &payee_admin,
            &soroban_sdk::String::from_str(&e, "PayeeToken"),
            &soroban_sdk::String::from_str(&e, "PAY"),
            &7u32,
        );

        let mut recurring_id = 0u32;
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), 1_000);
            crate::balance::increase_supply(&e, 1_000);

            // Use the payee contract address as the payee
            recurring_id = setup_recurring(&e, payer.clone(), payee_contract_id.clone(), 500, 100);
        });

        e.as_contract(&contract_id, || {
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, recurring_id);
        });

        // Verify the payee contract received the tokens
        assert_eq!(payee_client.balance(&payee_contract_id), 500i128);
    }

    // --- Event content tests ---

    // Verifies that setup_recurring emits ("recur_stp", payer) topics and
    // (payee, amount) as tuple data.
    #[test]
    fn test_setup_recurring_event_topics_and_data() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, payee, _id) = fund_and_setup(&e, &contract_id, 500, 100);

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        let event = events.last().unwrap();
        let topics = event.1;
        assert_eq!(topics.len(), 2);
        let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::symbol_short!("recur_stp"));
        let t1 = Address::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
        assert_eq!(t1, payer);
        // data is (payee, amount) encoded as Vec<Val>
        let data_vec =
            soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&e, &event.2).unwrap();
        let data_payee = Address::try_from_val(&e, &data_vec.get(0).unwrap()).unwrap();
        assert_eq!(data_payee, payee);
        let data_amount = i128::try_from_val(&e, &data_vec.get(1).unwrap()).unwrap();
        assert_eq!(data_amount, 500);
    }

    // Verifies that execute_recurring emits ("recur_exe", recurring_id) topics
    // and amount as scalar data.
    #[test]
    fn test_execute_recurring_event_topics_and_data() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        let before = e.events().all().len();

        e.as_contract(&contract_id, || {
            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);
        });

        let events = e.events().all();
        assert_eq!(events.len(), before + 1);
        let event = events.last().unwrap();
        let topics = event.1;
        assert_eq!(topics.len(), 2);
        let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::symbol_short!("recur_exe"));
        let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
        assert_eq!(t1, id);
        let data_amount = i128::try_from_val(&e, &event.2).unwrap();
        assert_eq!(data_amount, 500);
    }

    // Verifies that cancel_recurring emits ("recur_cxl", recurring_id, caller)
    // topics and unit () as data.
    #[test]
    fn test_cancel_recurring_event_topics_and_data() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        let before = e.events().all().len();

        e.as_contract(&contract_id, || {
            cancel_recurring(&e, payer.clone(), id);
        });

        let events = e.events().all();
        assert_eq!(events.len(), before + 1);
        let event = events.last().unwrap();
        let topics = event.1;
        assert_eq!(topics.len(), 3);
        let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::symbol_short!("recur_cxl"));
        let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
        assert_eq!(t1, id);
        let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
        assert_eq!(t2, payer);
        // data is () (unit/void) — assert it is the void Val
        assert!(event.2.is_void());
    }

    #[test]
    fn test_transfer_recurring_payer_updates_record_and_indexes() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (old_payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        let new_payer = Address::generate(&e);

        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, new_payer.clone(), 500);
            transfer_recurring_payer(&e, old_payer.clone(), new_payer.clone(), id);

            let record = get_recurring(&e, id);
            assert_eq!(record.payer, new_payer.clone());
            assert_eq!(record.payee, payee);

            let old_ids = get_recurring_by_payer(&e, old_payer.clone());
            let new_ids = get_recurring_by_payer(&e, new_payer.clone());
            assert_eq!(old_ids.len(), 0);
            assert_eq!(new_ids.len(), 1);
            assert_eq!(new_ids.get(0).unwrap(), id);
        });
    }

    #[test]
    fn test_transfer_recurring_payer_event_topics_and_data() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (old_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        let new_payer = Address::generate(&e);

        let before = e.events().all().len();
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, new_payer.clone(), 500);
            transfer_recurring_payer(&e, old_payer.clone(), new_payer.clone(), id);
        });

        let events = e.events().all();
        assert_eq!(events.len(), before + 1);
        let event = events.last().unwrap();
        let topics = event.1;
        assert_eq!(topics.len(), 3);
        let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::Symbol::new(&e, "recur_xfer_p"));
        let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
        assert_eq!(t1, id);
        let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
        assert_eq!(t2, new_payer);
        assert!(event.2.is_void());
    }

    #[test]
    fn test_execute_recurring_after_payer_transfer_charges_new_payer() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (old_payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        let new_payer = Address::generate(&e);

        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, new_payer.clone(), 500);
            transfer_recurring_payer(&e, old_payer.clone(), new_payer.clone(), id);

            let seq = e.ledger().sequence();
            e.ledger().with_mut(|l| l.sequence_number = seq + 101);
            execute_recurring(&e, id);

            assert_eq!(read_balance(&e, old_payer.clone()), 500);
            assert_eq!(read_balance(&e, new_payer.clone()), 0);
            assert_eq!(read_balance(&e, payee.clone()), 500);
        });
    }

    // --- Issue #275: Cancel recurring preserves payer balance ---
    // Since setup_recurring uses a "pull" model (funds not locked at setup),
    // canceling a recurring payment does not require a refund - the payer's
    // balance remains unchanged because funds were never transferred.

    // Issue #445: Execution boundary tests

    // Exact boundary: advancing ledger by exactly interval must succeed.
    #[test]
    fn test_execute_on_exact_boundary_succeeds() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            let r = get_recurring(&e, id);
            let due = r.last_charged_ledger + r.interval;
            e.ledger().with_mut(|l| l.sequence_number = due);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 500);
            assert_eq!(read_balance(&e, payer.clone()), 0);
        });
    }

    // One ledger before boundary must be rejected.
    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "interval has not elapsed")]
    fn test_execute_one_ledger_before_boundary_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            let r = get_recurring(&e, id);
            let one_before = r.last_charged_ledger + r.interval - 1;
            e.ledger().with_mut(|l| l.sequence_number = one_before);
            execute_recurring(&e, id);
        });
    }

    // One ledger after boundary must succeed.
    #[test]
    fn test_execute_one_ledger_after_boundary_succeeds() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            let r = get_recurring(&e, id);
            let one_after = r.last_charged_ledger + r.interval + 1;
            e.ledger().with_mut(|l| l.sequence_number = one_after);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 500);
        });
    }

    // Delayed execution: last_charged_ledger must anchor to the scheduled
    // due date (old_last_charged + interval), not the actual current_ledger.
    #[test]
    fn test_execute_updates_last_charged_to_scheduled_not_actual_ledger() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            let r = get_recurring(&e, id);
            let old_last = r.last_charged_ledger;
            let interval = r.interval;
            // Execute 5 ledgers late
            e.ledger().with_mut(|l| l.sequence_number = old_last + interval + 5);
            execute_recurring(&e, id);
            let updated = get_recurring(&e, id);
            // Must be anchored to schedule, not current ledger
            assert_eq!(updated.last_charged_ledger, old_last + interval);
        });
    }

    // Three consecutive executions must each fire on the correct scheduled ledger.
    #[test]
    fn test_multiple_executions_maintain_consistent_schedule() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        // Fund payer for 3 charges
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), 1_000);
        });

        e.as_contract(&contract_id, || {
            let r = get_recurring(&e, id);
            let base = r.last_charged_ledger;
            let interval = r.interval;

            // First execution — exactly on schedule
            e.ledger().with_mut(|l| l.sequence_number = base + interval);
            execute_recurring(&e, id);
            assert_eq!(get_recurring(&e, id).last_charged_ledger, base + interval);
            assert_eq!(read_balance(&e, payee.clone()), 500);

            // Second execution — exactly on next schedule
            e.ledger().with_mut(|l| l.sequence_number = base + interval * 2);
            execute_recurring(&e, id);
            assert_eq!(get_recurring(&e, id).last_charged_ledger, base + interval * 2);
            assert_eq!(read_balance(&e, payee.clone()), 1_000);

            // Third execution — exactly on third schedule
            e.ledger().with_mut(|l| l.sequence_number = base + interval * 3);
            execute_recurring(&e, id);
            assert_eq!(get_recurring(&e, id).last_charged_ledger, base + interval * 3);
            assert_eq!(read_balance(&e, payee.clone()), 1_500);
        });
    }
}
