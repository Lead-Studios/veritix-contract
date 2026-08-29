#[cfg(test)]
mod balance_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::balance::{
        decrease_supply, increase_supply, read_balance, read_total_supply, receive_balance,
        spend_balance,
    };
    use crate::contract::VeritixToken;

    fn setup_env() -> (Env, Address) {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, VeritixToken);
        (e, contract_id)
    }

    #[test]
    fn test_read_balance_returns_zero_for_unknown_address() {
        let (e, contract_id) = setup_env();
        let addr = Address::generate(&e);
        e.as_contract(&contract_id, || {
            assert_eq!(read_balance(&e, addr), 0);
        });
    }

    #[test]
    fn test_receive_balance_sets_and_reads_correctly() {
        let (e, contract_id) = setup_env();
        let addr = Address::generate(&e);
        e.as_contract(&contract_id, || {
            receive_balance(&e, addr.clone(), 500);
            assert_eq!(read_balance(&e, addr), 500);
        });
    }

    #[test]
    fn test_spend_balance_decrements_correctly() {
        let (e, contract_id) = setup_env();
        let addr = Address::generate(&e);
        e.as_contract(&contract_id, || {
            receive_balance(&e, addr.clone(), 1_000);
            spend_balance(&e, addr.clone(), 400);
            assert_eq!(read_balance(&e, addr), 600);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "insufficient balance")]
    fn test_spend_balance_insufficient_panics() {
        let (e, contract_id) = setup_env();
        let addr = Address::generate(&e);
        e.as_contract(&contract_id, || {
            receive_balance(&e, addr.clone(), 100);
            spend_balance(&e, addr.clone(), 200);
        });
    }

    #[test]
    fn test_supply_increases_and_decreases_correctly() {
        let (e, contract_id) = setup_env();
        e.as_contract(&contract_id, || {
            assert_eq!(read_total_supply(&e), 0);
            increase_supply(&e, 1_000);
            assert_eq!(read_total_supply(&e), 1_000);
            decrease_supply(&e, 300);
            assert_eq!(read_total_supply(&e), 700);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "supply cannot be negative")]
    fn test_decrease_supply_below_zero_panics() {
        let (e, contract_id) = setup_env();
        e.as_contract(&contract_id, || {
            increase_supply(&e, 100);
            decrease_supply(&e, 200);
        });
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "supply overflow")]
    fn test_supply_overflow_panics() {
        let (e, contract_id) = setup_env();
        e.as_contract(&contract_id, || {
            increase_supply(&e, i128::MAX);
            increase_supply(&e, 1);
        });
    }
}
