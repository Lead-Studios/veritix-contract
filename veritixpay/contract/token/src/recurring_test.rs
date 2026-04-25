#[cfg(test)]
mod recurring_tests {
    use soroban_sdk::{
        testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Events as _},
        Address, Env, IntoVal,
    };

    use crate::balance::read_balance;
    use crate::contract::{VeritixToken, VeritixTokenClient};
    use crate::recurring::{cancel_recurring, execute_recurring, get_recurring, setup_recurring};

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

    #[test]
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

    #[test]
    fn test_execute_transfers_funds() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            // Advance ledger past the interval.
            e.ledger().set_sequence_number(e.ledger().sequence() + 101);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 500);
            assert_eq!(read_balance(&e, payer.clone()), 0);
        });
    }

    #[test]
    #[should_panic(expected = "interval has not elapsed")]
    fn test_execute_too_early_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            // Only advance by 50 — not enough.
            e.ledger().set_sequence_number(e.ledger().sequence() + 50);
            execute_recurring(&e, id);
        });
    }

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

    #[test]
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

    #[test]
    #[should_panic(expected = "recurring payment is not active")]
    fn test_execute_after_cancel_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        e.as_contract(&contract_id, || {
            cancel_recurring(&e, payer.clone(), id);
            e.ledger().set_sequence_number(e.ledger().sequence() + 200);
            execute_recurring(&e, id);
        });
    }

    #[test]
    #[should_panic(expected = "InsufficientBalance")]
    fn test_execute_recurring_insufficient_balance_panics() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        // Fund payer with less than the recurring amount.
        let (payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);
        e.as_contract(&contract_id, || {
            // Drain the payer balance so they can no longer cover the charge.
            crate::balance::spend_balance(&e, payer.clone(), 500);
            e.ledger().set_sequence_number(e.ledger().sequence() + 101);
            execute_recurring(&e, id);
        });
    }

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

            e.ledger().set_sequence_number(start + 101);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 1_000);

            e.ledger().set_sequence_number(start + 202);
            execute_recurring(&e, id);
            assert_eq!(read_balance(&e, payee.clone()), 2_000);
        });
    }

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

        assert_eq!(
            e.auths(),
            std::vec![
                (
                    payer.clone(),
                    AuthorizedInvocation {
                        function: AuthorizedFunction::Contract((
                            client.address.clone(),
                            soroban_sdk::symbol_short!("setup_recurring"),
                            (&payer, &payee, amount, interval).into_val(&e),
                        )),
                        sub_invocations: std::vec![],
                    }
                ),
                (
                    payee.clone(),
                    AuthorizedInvocation {
                        function: AuthorizedFunction::Contract((
                            client.address.clone(),
                            soroban_sdk::symbol_short!("setup_recurring"),
                            (&payer, &payee, amount, interval).into_val(&e),
                        )),
                        sub_invocations: std::vec![],
                    }
                ),
            ]
        );
    }

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

            e.ledger().set_sequence_number(e.ledger().sequence() + 101);
            execute_recurring(&e, id);
            assert_invariant();

            // Fund payer for a second execution
            crate::balance::receive_balance(&e, payer.clone(), 1_000);
            crate::balance::increase_supply(&e, 1_000);
            assert_invariant();

            e.ledger().set_sequence_number(e.ledger().sequence() + 101);
            execute_recurring(&e, id);
            assert_invariant();
        });
    }

    // --- Issue #162: Event emission tests ---

    #[test]
    fn test_setup_recurring_emits_event() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, _id) = fund_and_setup(&e, &contract_id, 500, 100);

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        // Topics: (recurring_setup, payer), data: (payee, amount)
        assert_eq!(events.first().unwrap().0.len(), 2);
    }

    #[test]
    fn test_execute_recurring_emits_event() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let (_payer, _payee, id) = fund_and_setup(&e, &contract_id, 500, 100);

        // Clear setup event
        let _ = e.events().all();

        e.as_contract(&contract_id, || {
            e.ledger().set_sequence_number(e.ledger().sequence() + 101);
            execute_recurring(&e, id);
        });

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        // Topics: (recurring_executed, recurring_id), data: amount
        assert_eq!(events.first().unwrap().0.len(), 2);
    }

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
        assert_eq!(events.first().unwrap().0.len(), 3);
    }
}

// --- Issue #171: i128 boundary tests ---

#[cfg(test)]
mod recurring_boundary_tests {
    use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env};

    use crate::contract::VeritixToken;
    use crate::recurring::{execute_recurring, setup_recurring};

    fn setup_env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    #[test]
    fn test_recurring_minimum_interval() {
        let e = setup_env();
        let contract_id = e.register_contract(None, VeritixToken);
        let payer = Address::generate(&e);
        let payee = Address::generate(&e);
        let amount = 100i128;

        let mut id = 0u32;
        e.as_contract(&contract_id, || {
            crate::balance::receive_balance(&e, payer.clone(), amount * 3);
            // interval = 1: executes on every ledger advance
            id = setup_recurring(&e, payer.clone(), payee.clone(), amount, 1);
        });

        // Advance ledger by 1 and execute
        e.ledger().with_mut(|l| l.sequence_number += 1);
        e.as_contract(&contract_id, || {
            execute_recurring(&e, id);
            assert_eq!(crate::balance::read_balance(&e, payee.clone()), amount);
        });

        // Advance again and execute a second time
        e.ledger().with_mut(|l| l.sequence_number += 1);
        e.as_contract(&contract_id, || {
            execute_recurring(&e, id);
            assert_eq!(crate::balance::read_balance(&e, payee.clone()), amount * 2);
        });
    }
}
