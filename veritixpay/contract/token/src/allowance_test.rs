#[cfg(test)]
mod allowance_tests {
    use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env};

    use crate::allowance::{get_allowances_for_spender, read_allowance, spend_allowance, write_allowance};
    use crate::contract::VeritixToken;

    fn setup_env() -> (Env, Address) {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, VeritixToken);
        (e, contract_id)
    }

    #[test]
    fn test_approve_stores_allowance() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 100;
            write_allowance(&e, from.clone(), spender.clone(), 500, expiry);
            let a = read_allowance(&e, from, spender);
            assert_eq!(a.amount, 500);
            assert_eq!(a.expiration_ledger, expiry);
        });
    }

    #[test]
    fn test_approve_overwrites_existing_allowance() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 100;
            write_allowance(&e, from.clone(), spender.clone(), 500, expiry);
            write_allowance(&e, from.clone(), spender.clone(), 200, expiry);
            let a = read_allowance(&e, from, spender);
            assert_eq!(a.amount, 200);
        });
    }

    #[test]
    fn test_spend_allowance_decrements_correctly() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 100;
            write_allowance(&e, from.clone(), spender.clone(), 500, expiry);
            spend_allowance(&e, from.clone(), spender.clone(), 200);
            let a = read_allowance(&e, from, spender);
            assert_eq!(a.amount, 300);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "insufficient allowance")]
    fn test_spend_allowance_more_than_allowed_panics() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 100;
            write_allowance(&e, from.clone(), spender.clone(), 100, expiry);
            spend_allowance(&e, from.clone(), spender.clone(), 200);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "allowance is expired")]
    fn test_spend_expired_allowance_panics() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 5;
            write_allowance(&e, from.clone(), spender.clone(), 500, expiry);
            // Advance ledger past expiry
            e.ledger().with_mut(|l| l.sequence_number = expiry + 1);
            spend_allowance(&e, from.clone(), spender.clone(), 100);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "expiration ledger is in the past")]
    fn test_approve_with_past_ledger_panics() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            e.ledger().with_mut(|l| l.sequence_number = 100);
            // expiration_ledger is before current ledger
            write_allowance(&e, from.clone(), spender.clone(), 500, 50);
        });
    }

    #[test]
    fn test_approve_zero_amount_clears_allowance() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 100;
            write_allowance(&e, from.clone(), spender.clone(), 500, expiry);
            write_allowance(&e, from.clone(), spender.clone(), 0, expiry);
            let a = read_allowance(&e, from, spender);
            assert_eq!(a.amount, 0);
        });
    }

    #[test]
    fn test_allowances_for_spender_index_adds_and_removes_from() {
        let (e, contract_id) = setup_env();
        let from = Address::generate(&e);
        let spender = Address::generate(&e);
        e.as_contract(&contract_id, || {
            let expiry = e.ledger().sequence() + 100;
            write_allowance(&e, from.clone(), spender.clone(), 500, expiry);
            let indexed = get_allowances_for_spender(&e, spender.clone());
            assert_eq!(indexed.len(), 1);
            assert_eq!(indexed.get(0).unwrap(), from.clone());

            write_allowance(&e, from.clone(), spender.clone(), 0, expiry);
            let indexed_after = get_allowances_for_spender(&e, spender);
            assert_eq!(indexed_after.len(), 0);
        });
    }
}
