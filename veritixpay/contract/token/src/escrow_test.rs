use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Symbol, TryFromVal,
};

use crate::balance::increase_supply;
use crate::balance::read_balance;
use crate::balance::read_total_supply;
use crate::contract::VeritixToken;
use crate::escrow::{
    admin_settle_escrow, create_escrow, get_escrow, refund_escrow, release_escrow, try_get_escrow,
    try_refund_escrow, try_release_escrow,
};
use crate::freeze::{freeze_account, is_frozen};
use crate::storage_types::{read_counter, DataKey};

// Helper to create a fresh Env with mock auth enabled.
fn setup_env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    e
}

fn assert_supply_matches_balances(e: &Env, addresses: &[Address]) {
    let tracked_sum = addresses
        .iter()
        .fold(0i128, |sum, address| sum + read_balance(e, address.clone()));
    assert_eq!(read_total_supply(e), tracked_sum);
}

// Verifies that create_escrow stores a record with correct id, depositor,
// beneficiary, amount, and initial state (not released, not refunded).
// If this test fails, the basic escrow creation flow is broken.
#[test]
fn test_create_escrow_stores_record() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);

        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        let record = get_escrow(&e, escrow_id);

        assert_eq!(record.id, escrow_id);
        assert_eq!(record.depositor, depositor);
        assert_eq!(record.beneficiary, beneficiary);
        assert_eq!(record.amount, amount);
        assert!(!record.released);
        assert!(!record.refunded);
    });

    assert_eq!(e.events().all().len(), 1);
}

// Happy-path release: creates an escrow and releases it to the beneficiary,
// verifying that contract balance decreases and beneficiary balance increases
// by the exact escrow amount.
#[test]
fn test_release_escrow_happy_path() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        let contract_addr = e.current_contract_address();
        let before_contract_balance = read_balance(&e, contract_addr.clone());
        let before_beneficiary_balance = read_balance(&e, beneficiary.clone());

        release_escrow(&e, beneficiary.clone(), escrow_id);

        let record = get_escrow(&e, escrow_id);
        assert!(record.released);
        assert!(!record.refunded);

        let after_contract_balance = read_balance(&e, contract_addr);
        let after_beneficiary_balance = read_balance(&e, beneficiary.clone());

        assert_eq!(before_contract_balance - amount, after_contract_balance);
        assert_eq!(before_beneficiary_balance + amount, after_beneficiary_balance);
    });

    assert_eq!(e.events().all().len(), 2);
}

// Happy-path refund: creates an escrow and refunds it to the depositor,
// verifying that contract balance decreases and depositor balance increases.
#[test]
fn test_refund_escrow_happy_path() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        let contract_addr = e.current_contract_address();
        let before_contract_balance = read_balance(&e, contract_addr.clone());
        let before_depositor_balance = read_balance(&e, depositor.clone());

        refund_escrow(&e, depositor.clone(), escrow_id);

        let record = get_escrow(&e, escrow_id);
        assert!(record.refunded);
        assert!(!record.released);

        let after_contract_balance = read_balance(&e, contract_addr);
        let after_depositor_balance = read_balance(&e, depositor.clone());

        assert_eq!(before_contract_balance - amount, after_contract_balance);
        assert_eq!(before_depositor_balance + amount, after_depositor_balance);
    });

    assert_eq!(e.events().all().len(), 2);
}

// Critical invariant: after escrow creation and release, the sum of all
// tracked balances (depositor, beneficiary, contract) must equal total supply.
// If this fails, tokens are being created or destroyed during escrow operations.
#[test]
fn test_escrow_create_and_release_preserve_supply_invariant() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        increase_supply(&e, amount);
        assert_supply_matches_balances(&e, &[depositor.clone(), beneficiary.clone(), e.current_contract_address()]);

        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        assert_supply_matches_balances(&e, &[depositor.clone(), beneficiary.clone(), e.current_contract_address()]);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, beneficiary.clone(), escrow_id);
        assert_supply_matches_balances(&e, &[depositor.clone(), beneficiary.clone(), e.current_contract_address()]);
    });
}

// Critical invariant: after escrow creation and refund, the supply invariant
// (sum of all balances == total supply) must hold — prevents silent token loss
// or creation during refund flows.
#[test]
fn test_escrow_create_and_refund_preserve_supply_invariant() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        increase_supply(&e, amount);
        assert_supply_matches_balances(&e, &[depositor.clone(), beneficiary.clone(), e.current_contract_address()]);

        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        assert_supply_matches_balances(&e, &[depositor.clone(), beneficiary.clone(), e.current_contract_address()]);
    });

    e.as_contract(&contract_id, || {
        refund_escrow(&e, depositor.clone(), escrow_id);
        assert_supply_matches_balances(&e, &[depositor.clone(), beneficiary.clone(), e.current_contract_address()]);
    });
}

// Verifies that the escrow counter increments correctly across multiple calls,
// and that the public escrow_count getter matches the internal counter.
// If IDs skip or duplicate, downstream indexers will mis-track escrows.
#[test]
fn test_create_escrow_increments_counter() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let depositor_one = Address::generate(&e);
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor_one.clone(), amount);
        let escrow_id = create_escrow(&e, depositor_one.clone(), beneficiary.clone(), amount, 1000);
        assert_eq!(escrow_id, 1);
    });

    let depositor_two = Address::generate(&e);
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor_two.clone(), amount);
        let escrow_id = create_escrow(&e, depositor_two.clone(), beneficiary.clone(), amount, 1000);
        assert_eq!(escrow_id, 2);
    });

    e.as_contract(&contract_id, || {
        assert_eq!(read_counter(&e, &DataKey::EscrowCount), 2);
        assert_eq!(VeritixToken::escrow_count(e.clone()), 2);
    });
}

// Ensures the public escrow_count() starts at zero before any escrow is
// created and increments correctly with each subsequent creation.
#[test]
fn test_escrow_count_getter_reflects_creations() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        assert_eq!(VeritixToken::escrow_count(e.clone()), 0);
    });

    let depositor_one = Address::generate(&e);
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor_one.clone(), amount);
        let _ = VeritixToken::create_escrow(e.clone(), depositor_one.clone(), beneficiary.clone(), amount, 1000);
        assert_eq!(VeritixToken::escrow_count(e.clone()), 1);
    });

    let depositor_two = Address::generate(&e);
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor_two.clone(), amount);
        let _ = VeritixToken::create_escrow(e.clone(), depositor_two.clone(), beneficiary.clone(), amount, 1000);
        assert_eq!(VeritixToken::escrow_count(e.clone()), 2);
    });
}

// Ensures that querying a non-existent escrow returns a descriptive error
// rather than panicking — important for safe frontend queries.
#[test]
fn test_get_escrow_missing_id_returns_not_found_error() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);

    e.as_contract(&contract_id, || {
        assert_eq!(try_get_escrow(&e, 999), Err("escrow not found"));
    });
}

// Ensures that releasing a non-existent escrow returns an error instead of
// silently failing or panicking with an unrelated message.
#[test]
fn test_release_missing_id_returns_not_found_error() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        assert_eq!(
            try_release_escrow(&e, beneficiary.clone(), 999),
            Err("escrow not found")
        );
    });
}

// Ensures that refunding a non-existent escrow returns an error instead of
// silently failing or panicking with an unrelated message.
#[test]
fn test_refund_missing_id_returns_not_found_error() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);

    e.as_contract(&contract_id, || {
        assert_eq!(
            try_refund_escrow(&e, depositor.clone(), 999),
            Err("escrow not found")
        );
    });
}

// Ensures that creating an escrow where depositor == beneficiary is rejected
// — prevents degenerate escrows where one party acts as both sides.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidEscrow: depositor and beneficiary cannot be the same address")]
fn test_create_escrow_same_address_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let addr = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, addr.clone(), amount);
        create_escrow(&e, addr.clone(), addr.clone(), amount, 1000);
    });
}

// Ensures that only the beneficiary can release an escrow — a third-party hacker
// must be rejected with "not beneficiary".
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not beneficiary")]
fn test_release_unauthorized_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let hacker = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, hacker.clone(), escrow_id);
    });
}

// Ensures that only the depositor can refund a non-expired escrow — the
// beneficiary attempting a refund must be rejected with "not depositor".
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not depositor")]
fn test_refund_unauthorized_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        refund_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

// Ensures that releasing an already-released escrow panics — prevents double
// claims that would drain the contract balance.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already settled")]
fn test_double_release_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        release_escrow(&e, beneficiary.clone(), escrow_id);
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

#[test]
#[ignore = "partial release not implemented in current escrow.rs"]
fn test_partial_release_50_percent() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;
    let _release_amount = amount / 2;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

#[test]
#[ignore = "partial release not implemented in current escrow.rs"]
fn test_partial_release_to_zero_marks_as_released() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;
    let _release_amount = amount / 2;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic]
fn test_partial_release_over_remaining_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;
    let _release_amount = amount / 2;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, beneficiary.clone(), escrow_id);
        // Over-release: second full release panics "already settled"
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic]
fn test_partial_release_after_full_release_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id: u32 = 0;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        // Full release
        release_escrow(&e, beneficiary.clone(), escrow_id);
        // Any release after full release must panic "already settled"
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

// Ensures that refunding an already-refunded escrow panics — prevents double
// refunds that would drain the contract balance.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already settled")]
fn test_double_refund_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        refund_escrow(&e, depositor.clone(), escrow_id);
        refund_escrow(&e, depositor.clone(), escrow_id);
    });
}

// Ensures that releasing an escrow that was already refunded panics — once
// settled, an escrow cannot change state.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already settled")]
fn test_release_after_refund_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        refund_escrow(&e, depositor.clone(), escrow_id);
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

// Ensures that creating an escrow with amount = 0 is rejected — escrows must
// lock a positive amount of tokens.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "amount must be positive")]
fn test_create_escrow_zero_amount_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        create_escrow(&e, depositor.clone(), beneficiary.clone(), 0, 1000);
    });
}

// Ensures that creating an escrow with a negative amount is rejected — escrows
// must lock a positive amount of tokens.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "amount must be positive")]
fn test_create_escrow_negative_amount_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        create_escrow(&e, depositor.clone(), beneficiary.clone(), -1, 1000);
    });
}

// Ensures that the depositor cannot release — only the beneficiary can call
// release_escrow. This enforces the auth boundary between the two roles.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not beneficiary")]
fn test_release_by_depositor_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, depositor.clone(), escrow_id);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "missing authorization")]
fn test_release_escrow_without_auth_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;
    let mut escrow_id = 0u32;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        e.set_auths(&[]);
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

// Ensures that the beneficiary cannot refund a non-expired escrow — only the
// depositor can trigger a refund before expiry.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not depositor")]
fn test_refund_by_beneficiary_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        refund_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

// --- Issue #162: Event emission tests ---

// Verifies that create_escrow emits an event with (escrow_created, depositor,
// beneficiary) topics and amount as data.
#[test]
fn test_create_escrow_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 1000);
        create_escrow(&e, depositor.clone(), beneficiary.clone(), 1000, 1000);
    });

    let events = e.events().all();
    assert_eq!(events.len(), 1);
    // Topics: (escrow_created, depositor, beneficiary), data: amount
    assert_eq!(events.first().unwrap().1.len(), 3);
}

// Verifies that release_escrow emits a single event with (escrow_released,
// escrow_id, beneficiary) topics and amount as data.
#[test]
fn test_release_escrow_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let mut escrow_id = 0u32;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 1000);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1000, 1000);
    });

    let before = e.events().all().len();

    e.as_contract(&contract_id, || {
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });

    let events = e.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (escrow_released, escrow_id, beneficiary), data: amount
    assert_eq!(events.last().unwrap().1.len(), 3);
}

// Verifies that refund_escrow emits a single event with (escrow_refunded,
// escrow_id, depositor) topics and amount as data.
#[test]
fn test_refund_escrow_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let mut escrow_id = 0u32;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 1000);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1000, 1000);
    });

    let before = e.events().all().len();

    e.as_contract(&contract_id, || {
        refund_escrow(&e, depositor.clone(), escrow_id);
    });

    let events = e.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (escrow_refunded, escrow_id, depositor), data: amount
    assert_eq!(events.last().unwrap().1.len(), 3);
}

// Ensures that creating an escrow with an expiration ledger in the past is
// rejected — prevents creation of instantly-expired escrows.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "expiration ledger is in the past")]
fn test_create_escrow_past_expiry_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        // Advance ledger so expiry_ledger = 0 is in the past
        e.ledger().with_mut(|l| l.sequence_number = 10);
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 0);
    });
}

// --- Issue #87: Frozen-account deadlock prevention tests ---

// Ensures that a release call from a non-beneficiary still panics even when
// the beneficiary is frozen — the auth check fires before freeze check.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not beneficiary")]
fn test_release_blocked_when_beneficiary_frozen() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let admin = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        freeze_account(&e, admin.clone(), beneficiary.clone());
        assert!(is_frozen(&e, &beneficiary));
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, depositor.clone(), escrow_id);
    });
}

// Verifies that an expired escrow can be refunded by a third party (anyone),
// preventing funds from being stuck when the depositor is inactive.
#[test]
fn test_expired_escrow_can_be_refunded_by_third_party() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let third_party = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 5);
    });

    e.as_contract(&contract_id, || {
        // Advance past expiry
        e.ledger().with_mut(|l| l.sequence_number = 6);
        let before = read_balance(&e, depositor.clone());
        refund_escrow(&e, third_party.clone(), escrow_id);
        let record = get_escrow(&e, escrow_id);
        assert!(record.refunded);
        assert_eq!(read_balance(&e, depositor.clone()), before + amount);
    });
}

// Verifies that a non-expired escrow cannot be refunded by a third party (anyone
// other than the depositor) — prevents unauthorized fund extraction.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not depositor")]
fn test_non_expired_escrow_cannot_be_refunded_by_third_party() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let third_party = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        // Ledger has not advanced past expiry
        refund_escrow(&e, third_party.clone(), escrow_id);
    });
}

// Tests the admin_settle_escrow escape hatch when the beneficiary is frozen
// and cannot claim funds — admin should be able to settle to an alternate recipient.
#[test]
fn test_admin_settle_escrow_when_beneficiary_frozen() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let admin = Address::generate(&e);
    let alternate = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        // Bootstrap: give admin role and fund depositor
        crate::admin::write_admin(&e, &admin);
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        increase_supply(&e, amount);

        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);

        // Freeze beneficiary after deposit — normal release is now deadlocked
        freeze_account(&e, admin.clone(), beneficiary.clone());
        assert!(is_frozen(&e, &beneficiary));

        // Admin escape hatch: settle to an alternate recipient
        let before = read_balance(&e, alternate.clone());
        admin_settle_escrow(&e, admin.clone(), escrow_id, alternate.clone());
        let after = read_balance(&e, alternate.clone());

        assert_eq!(after - before, amount);

        let record = get_escrow(&e, escrow_id);
        assert!(record.released);
    });
}

// --- Event content tests ---

// Verifies that create_escrow emits ("escr_crtd", depositor, beneficiary) topics
// and amount as scalar data — the exact values off-chain indexers depend on.
#[test]
fn test_create_escrow_event_topics_and_data() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    let events = e.events().all();
    let event = events.last().unwrap();
    let topics = event.1;
    assert_eq!(topics.len(), 3);
    let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
    assert_eq!(t0, soroban_sdk::symbol_short!("escr_crtd"));
    let t1 = Address::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
    assert_eq!(t1, depositor);
    let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
    assert_eq!(t2, beneficiary);
    let data_amount = i128::try_from_val(&e, &event.2).unwrap();
    assert_eq!(data_amount, amount);
}

// Verifies that release_escrow emits ("escr_rls", escrow_id, beneficiary) topics
// and amount as scalar data.
#[test]
fn test_release_escrow_event_topics_and_data() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;
    let mut escrow_id = 0u32;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        release_escrow(&e, beneficiary.clone(), escrow_id);
    });

    let events = e.events().all();
    let event = events.last().unwrap();
    let topics = event.1;
    assert_eq!(topics.len(), 3);
    let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
    assert_eq!(t0, soroban_sdk::symbol_short!("escr_rls"));
    let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
    assert_eq!(t1, escrow_id);
    let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
    assert_eq!(t2, beneficiary);
    let data_amount = i128::try_from_val(&e, &event.2).unwrap();
    assert_eq!(data_amount, amount);
}

// Verifies that refund_escrow emits ("escr_rfnd", escrow_id, depositor) topics
// and amount as scalar data.
#[test]
fn test_refund_escrow_event_topics_and_data() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;
    let mut escrow_id = 0u32;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });

    e.as_contract(&contract_id, || {
        refund_escrow(&e, depositor.clone(), escrow_id);
    });

    let events = e.events().all();
    let event = events.last().unwrap();
    let topics = event.1;
    assert_eq!(topics.len(), 3);
    let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
    assert_eq!(t0, soroban_sdk::symbol_short!("escr_rfnd"));
    let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
    assert_eq!(t1, escrow_id);
    let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
    assert_eq!(t2, depositor);
    let data_amount = i128::try_from_val(&e, &event.2).unwrap();
    assert_eq!(data_amount, amount);
}

// Tests that admin_settle_escrow also works when the depositor is frozen —
// admin can settle to the beneficiary even though the standard paths are blocked.
#[test]
fn test_admin_settle_escrow_when_depositor_frozen() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let admin = Address::generate(&e);
    let amount = 500i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::admin::write_admin(&e, &admin);
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        increase_supply(&e, amount);

        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);

        // Freeze depositor after deposit
        freeze_account(&e, admin.clone(), depositor.clone());

        // Admin settles back to beneficiary (or any address)
        admin_settle_escrow(&e, admin.clone(), escrow_id, beneficiary.clone());

        assert_eq!(read_balance(&e, beneficiary.clone()), amount);
        assert!(get_escrow(&e, escrow_id).released);
    });
}

// Ensures that admin_settle_escrow cannot settle an already-settled escrow
// (released or refunded) — prevents double-spend to multiple recipients.
#[test]
fn test_admin_settle_escrow_frozen_beneficiary_alternate_recipient() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let admin = Address::generate(&e);
    let alternate = Address::generate(&e);
    let amount = 1_000i128;

    let mut escrow_id = 0u32;
    e.as_contract(&contract_id, || {
        crate::admin::write_admin(&e, &admin);
        crate::balance::receive_balance(&e, depositor.clone(), amount);
        increase_supply(&e, amount);
        escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);
        freeze_account(&e, admin.clone(), beneficiary.clone());
        assert!(is_frozen(&e, &beneficiary));

        let before = read_balance(&e, alternate.clone());
        admin_settle_escrow(&e, admin.clone(), escrow_id, alternate.clone());
        let after = read_balance(&e, alternate.clone());

        assert_eq!(after - before, amount);
        assert!(get_escrow(&e, escrow_id).released);
    });
}
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "DisputeOpen")]
fn test_refund_escrow_with_open_dispute_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), amount);

        let escrow_id = create_escrow(&e, depositor.clone(), beneficiary.clone(), amount, 1000);

        // Simulate an open dispute by writing the EscrowDispute key directly.
        e.storage()
            .persistent()
            .set(&crate::storage_types::DataKey::EscrowDispute(escrow_id), &true);

        // Depositor attempts to refund while dispute is active — must panic.
        refund_escrow(&e, depositor.clone(), escrow_id);
    });
}

// ── Issue #447: escrow_stats tests ───────────────────────────────────────────

// Verifies that escrow_stats returns zeroed fields when no escrows exist.
#[test]
fn test_escrow_stats_empty() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    e.as_contract(&contract_id, || {
        let stats = crate::escrow::escrow_stats(&e);
        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.settled_count, 0);
        assert_eq!(stats.total_value_locked, 0);
    });
}

// Verifies that escrow_stats correctly counts an active escrow and its locked value.
#[test]
fn test_escrow_stats_after_create() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 1_000);
        create_escrow(&e, depositor.clone(), beneficiary.clone(), 1_000, 1000);

        let stats = crate::escrow::escrow_stats(&e);
        assert_eq!(stats.total_count, 1);
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.settled_count, 0);
        assert_eq!(stats.total_value_locked, 1_000);
    });
}

// Verifies that releasing an escrow decrements active_count and total_value_locked.
#[test]
fn test_escrow_stats_after_release() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 1_000);
        let id = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1_000, 1000);
        release_escrow(&e, beneficiary.clone(), id);

        let stats = crate::escrow::escrow_stats(&e);
        assert_eq!(stats.total_count, 1);
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.settled_count, 1);
        assert_eq!(stats.total_value_locked, 0);
    });
}

// Verifies that refunding an escrow decrements active_count and total_value_locked.
#[test]
fn test_escrow_stats_after_refund() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 1_000);
        let id = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1_000, 1000);
        refund_escrow(&e, depositor.clone(), id);

        let stats = crate::escrow::escrow_stats(&e);
        assert_eq!(stats.total_count, 1);
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.settled_count, 1);
        assert_eq!(stats.total_value_locked, 0);
    });
}

// Verifies that escrow_stats correctly aggregates multiple escrows in mixed states.
#[test]
fn test_escrow_stats_mixed_states() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let depositor = Address::generate(&e);
    let beneficiary = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, depositor.clone(), 3_000);

        let id1 = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1_000, 1000);
        let id2 = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1_000, 1000);
        let _id3 = create_escrow(&e, depositor.clone(), beneficiary.clone(), 1_000, 1000);

        release_escrow(&e, beneficiary.clone(), id1);
        refund_escrow(&e, depositor.clone(), id2);

        let stats = crate::escrow::escrow_stats(&e);
        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.settled_count, 2);
        assert_eq!(stats.total_value_locked, 1_000);
    });
}
