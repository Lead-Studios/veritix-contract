#[cfg(test)]
mod validation_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    use crate::validation::{
        require_current_or_future_ledger, require_decimal_within_max, require_nonempty_string,
        require_not_frozen_account, require_positive_amount,
    };

    fn setup() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    // --- require_positive_amount ---

    #[test]
    fn test_require_positive_amount_accepts_positive() {
        require_positive_amount(1);
        require_positive_amount(i128::MAX);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_require_positive_amount_rejects_zero() {
        require_positive_amount(0);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_require_positive_amount_rejects_negative() {
        require_positive_amount(-1);
    }

    // --- require_nonempty_string ---

    #[test]
    fn test_require_nonempty_string_accepts_nonempty() {
        let e = setup();
        let s = String::from_str(&e, "hello");
        require_nonempty_string(&s, "must not be empty");
    }

    #[test]
    #[should_panic(expected = "must not be empty")]
    fn test_require_nonempty_string_rejects_empty() {
        let e = setup();
        let s = String::from_str(&e, "");
        require_nonempty_string(&s, "must not be empty");
    }

    // --- require_decimal_within_max ---

    #[test]
    fn test_require_decimal_within_max_accepts_eighteen() {
        require_decimal_within_max(18, 18);
    }

    #[test]
    #[should_panic(expected = "decimal exceeds maximum")]
    fn test_require_decimal_within_max_rejects_nineteen() {
        require_decimal_within_max(19, 18);
    }

    // --- require_current_or_future_ledger ---

    #[test]
    fn test_require_current_or_future_ledger_accepts_current() {
        // expiry == current ledger is valid
        require_current_or_future_ledger(100, 100);
    }

    #[test]
    #[should_panic(expected = "expiration ledger is in the past")]
    fn test_require_current_or_future_ledger_rejects_past() {
        require_current_or_future_ledger(100, 99);
    }

    // --- require_not_frozen_account ---

    #[test]
    fn test_require_not_frozen_account_accepts_unfrozen() {
        let e = setup();
        let contract_id = e.register_contract(None, crate::contract::VeritixToken);
        let addr = Address::generate(&e);
        e.as_contract(&contract_id, || {
            // Account is not frozen by default — should not panic
            require_not_frozen_account(&e, &addr);
        });
    }

    #[test]
    #[should_panic(expected = "account frozen")]
    fn test_require_not_frozen_account_rejects_frozen() {
        let e = setup();
        let contract_id = e.register_contract(None, crate::contract::VeritixToken);
        let addr = Address::generate(&e);
        e.as_contract(&contract_id, || {
            crate::freeze::freeze_account(&e, addr.clone(), addr.clone());
            require_not_frozen_account(&e, &addr);
        });
    }
}
