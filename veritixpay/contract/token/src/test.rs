use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events as _, Address, Env, String, Symbol};

use crate::contract::VeritixTokenClient;

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    (env, admin, user)
}

fn create_client_with_id(env: &Env) -> (Address, VeritixTokenClient<'_>) {
    let contract_id = env.register_contract(None, VeritixToken);
    (
        contract_id.clone(),
        VeritixTokenClient::new(env, &contract_id),
    )
}

fn create_client(env: &Env) -> VeritixTokenClient<'_> {
    create_client_with_id(env).1
}

fn initialize_client(client: &VeritixTokenClient<'_>, env: &Env, admin: &Address, decimal: u32) {
    client.initialize(
        admin,
        &String::from_str(env, "Veritix"),
        &String::from_str(env, "VTX"),
        &decimal,
    );
}

fn assert_supply_matches(client: &VeritixTokenClient<'_>, tracked_addresses: &[Address]) {
    let tracked_sum = tracked_addresses
        .iter()
        .fold(0i128, |sum, address| sum + client.balance(address));
    assert_eq!(client.total_supply(), tracked_sum);
}

#[test]
fn test_initialize() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);

    assert_eq!(client.admin(), admin);
    assert_eq!(client.name(), String::from_str(&env, "Veritix"));
    assert_eq!(client.symbol(), String::from_str(&env, "VTX"));
    assert_eq!(client.decimals(), 7u32);
}

#[test]
#[should_panic]
fn test_initialize_twice_panics() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    initialize_client(&client, &env, &admin, 7);
}

#[test]
#[should_panic]
fn test_initialize_rejects_decimal_above_eighteen() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 19);
}

#[test]
#[should_panic]
fn test_initialize_rejects_empty_name() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    client.initialize(
        &admin,
        &String::from_str(&env, ""),
        &String::from_str(&env, "VTX"),
        &7u32,
    );
}

#[test]
#[should_panic]
fn test_initialize_rejects_empty_symbol() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    client.initialize(
        &admin,
        &String::from_str(&env, "Veritix"),
        &String::from_str(&env, ""),
        &7u32,
    );
}

#[test]
#[should_panic]
fn test_initialize_requires_admin_authorization() {
    let (env, admin, _user) = setup();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
}

#[test]
fn test_mint() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);

    assert_eq!(client.balance(&user), 1000i128);
    assert_eq!(client.total_supply(), 1000i128);
}

#[test]
#[should_panic]
fn test_mint_unauthorized_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    env.set_auths(&[]);

    client.mint(&user, &user, &1000i128);
}

#[test]
fn test_burn() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);

    client.mint(&admin, &user, &1000i128);
    client.burn(&user, &500i128);

    assert_eq!(client.balance(&user), 500i128);
    assert_eq!(client.total_supply(), 500i128);
}

#[test]
#[should_panic]
fn test_burn_insufficient_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &100i128);

    client.burn(&user, &200i128);
}

#[test]
fn test_transfer() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.transfer(&user, &receiver, &400i128);

    assert_eq!(client.balance(&user), 600i128);
    assert_eq!(client.balance(&receiver), 400i128);
    assert_eq!(client.total_supply(), 1000i128);
}

#[test]
#[should_panic]
fn test_transfer_insufficient_balance_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);

    client.transfer(&user, &receiver, &100i128);
}

#[test]
fn test_transfer_from() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &500i128, &1000u32);
    client.transfer_from(&spender, &user, &receiver, &300i128);

    assert_eq!(client.balance(&receiver), 300i128);
    assert_eq!(client.allowance(&user, &spender), 200i128);
}

#[test]
fn test_approve_and_spend_allowance() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &400i128, &1000u32);
    client.transfer_from(&spender, &user, &spender, &200i128);

    assert_eq!(client.balance(&spender), 200i128);
    assert_eq!(client.allowance(&user, &spender), 200i128);
}

#[test]
fn test_transfer_from_spends_full_allowance_and_clears_it() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &400i128, &1000u32);
    client.transfer_from(&spender, &user, &receiver, &400i128);

    assert_eq!(client.balance(&receiver), 400i128);
    assert_eq!(client.allowance(&user, &spender), 0i128);
}

#[test]
fn test_allowance_expiration_equal_current_ledger_is_valid_for_current_ledger() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);
    let receiver = Address::generate(&env);
    let current_ledger = env.ledger().sequence();

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &250i128, &current_ledger);
    client.transfer_from(&spender, &user, &receiver, &250i128);

    assert_eq!(client.balance(&receiver), 250i128);
    assert_eq!(client.allowance(&user, &spender), 0i128);
}

#[test]
#[should_panic]
fn test_approve_with_past_expiration_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);

    // Advance ledger so that expiration_ledger = 0 is strictly in the past.
    env.ledger().set_sequence_number(10);
    client.approve(&user, &spender, &400i128, &0u32);
}

#[test]
#[should_panic]
fn test_expired_allowance_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &400i128, &0u32);

    client.transfer_from(&spender, &user, &spender, &100i128);
}

#[test]
fn test_admin_and_freeze_views_follow_state_changes() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let (contract_id, client) = create_client_with_id(&env);
    let new_admin = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);

    assert_eq!(client.admin(), admin);
    assert!(!client.is_frozen(&user));
    assert_eq!(
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<crate::storage_types::DataKey, bool>(&crate::storage_types::DataKey::Freeze(
                    user.clone(),
                ))
        }),
        None
    );

    client.set_admin(&new_admin);
    assert_eq!(client.admin(), new_admin);

    client.freeze(&user);
    assert!(client.is_frozen(&user));
    assert_eq!(
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<crate::storage_types::DataKey, bool>(&crate::storage_types::DataKey::Freeze(
                    user.clone(),
                ))
        }),
        Some(true)
    );

    client.unfreeze(&user);
    assert!(!client.is_frozen(&user));
    assert_eq!(
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<crate::storage_types::DataKey, bool>(&crate::storage_types::DataKey::Freeze(
                    user.clone(),
                ))
        }),
        None
    );
}

#[test]
fn test_clawback_reduces_total_supply() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);

    client.mint(&admin, &user, &1000i128);
    assert_eq!(client.balance(&user), 1000i128);
    assert_eq!(client.total_supply(), 1000i128);

    client.clawback(&admin, &user, &300i128);

    assert_eq!(client.balance(&user), 700i128);
    assert_eq!(client.total_supply(), 700i128);
}

#[test]
fn test_admin_freeze_and_clawback_smoke_via_client() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let new_admin = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1_000i128);

    client.set_admin(&new_admin);
    assert_eq!(client.admin(), new_admin);

    client.freeze(&user);
    assert!(client.is_frozen(&user));

    client.unfreeze(&user);
    assert!(!client.is_frozen(&user));

    client.clawback(&new_admin, &user, &250i128);
    assert_eq!(client.balance(&user), 750i128);
    assert_eq!(client.total_supply(), 750i128);
}

#[test]
fn test_core_token_operations_preserve_supply_invariant() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let receiver = Address::generate(&env);
    let tracked = [user.clone(), receiver.clone()];

    initialize_client(&client, &env, &admin, 7);

    client.mint(&admin, &user, &1_000i128);
    assert_supply_matches(&client, &tracked);

    client.transfer(&user, &receiver, &250i128);
    assert_supply_matches(&client, &tracked);

    client.burn(&receiver, &100i128);
    assert_supply_matches(&client, &tracked);

    client.clawback(&admin, &user, &150i128);
    assert_supply_matches(&client, &tracked);
}

#[test]
#[should_panic]
fn test_clawback_unauthorized_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    env.set_auths(&[]);

    client.clawback(&user, &user, &300i128);
}

#[test]
fn test_frozen_account_can_receive_from_escrow_release() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let beneficiary = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1_000i128);

    // Freeze the beneficiary — they should still be able to receive escrowed funds.
    client.freeze(&beneficiary);
    assert!(client.is_frozen(&beneficiary));

    let escrow_id = client.create_escrow(&user, &beneficiary, &1_000i128, &1000u32);

    // Release escrow to the frozen beneficiary — must not panic.
    client.release_escrow(&user, &escrow_id);

    assert_eq!(client.balance(&beneficiary), 1_000i128);
}

#[test]
fn test_freeze_and_unfreeze_emit_observable_events() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    let _ = env.events().all();

    client.freeze(&user);
    let freeze_events = env.events().all();
    assert_eq!(freeze_events.len(), 1);
    let freeze_event = freeze_events.first().unwrap();
    assert_eq!(freeze_event.0.len(), 2);
    assert_eq!(
        freeze_event.0.get(0).unwrap().into_val(&env),
        Symbol::new(&env, "frozen")
    );
    assert_eq!(
        freeze_event.0.get(1).unwrap().into_val(&env),
        user.clone().into()
    );
    assert_eq!(freeze_event.1.into_val(&env), admin.clone().into());

    let _ = env.events().all();

    client.unfreeze(&user);
    let unfreeze_events = env.events().all();
    assert_eq!(unfreeze_events.len(), 1);
    let unfreeze_event = unfreeze_events.first().unwrap();
    assert_eq!(unfreeze_event.0.len(), 2);
    assert_eq!(
        unfreeze_event.0.get(0).unwrap().into_val(&env),
        Symbol::new(&env, "unfrozen")
    );
    assert_eq!(
        unfreeze_event.0.get(1).unwrap().into_val(&env),
        user.clone().into()
    );
    assert_eq!(unfreeze_event.1.into_val(&env), admin.into());
}

#[test]
#[should_panic]
fn test_freeze_by_non_admin_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let attacker = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    env.set_auths(&[]);

    client.freeze(&attacker);
    let _ = user; // suppress unused warning
}

#[test]
#[should_panic]
fn test_transfer_from_over_allowance_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &100i128, &1000u32);

    // Attempt to spend more than the approved allowance.
    client.transfer_from(&spender, &user, &receiver, &200i128);
}

// --- Issue #162: Event emission tests ---

// NOTE: freeze/unfreeze currently emit no events — this is a known gap.
// The functions below test the events that ARE emitted.

#[test]
fn test_set_admin_emits_event() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let new_admin = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    let _ = env.events().all();

    client.set_admin(&new_admin);

    let events = env.events().all();
    assert_eq!(events.len(), 1);
    // Topics: (admin_set, current_admin), data: new_admin
    assert_eq!(events.first().unwrap().0.len(), 2);
}

// --- Issue #171: i128 boundary tests ---

#[test]
fn test_mint_i128_max_amount() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin, 7);

    client.mint(&admin, &user, &i128::MAX);
    assert_eq!(client.balance(&user), i128::MAX);
}

#[test]
#[should_panic(expected = "supply overflow")]
fn test_mint_overflow_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin, 7);

    client.mint(&admin, &user, &i128::MAX);
    client.mint(&admin, &user, &1i128);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_transfer_zero_amount_panics() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let (_, user2) = (Address::generate(&env), Address::generate(&env));
    let client = create_client(&env);
    initialize_client(&client, &env, &admin, 7);

    client.mint(&admin, &user, &1000i128);
    client.transfer(&user, &user2, &0i128);
}
