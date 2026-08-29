use super::*;
use crate::contract::VeritixTokenClient;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

// Helper functions for setup
pub(crate) fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    (env, admin, user)
}

pub(crate) fn create_client(env: &Env) -> VeritixTokenClient<'_> {
    let contract_id = env.register_contract(None, VeritixToken);
    VeritixTokenClient::new(env, &contract_id)
}

fn initialize_client(client: &VeritixTokenClient<'_>, env: &Env, admin: &Address) {
    client.initialize(
        admin,
        &String::from_str(env, "Veritix"),
        &String::from_str(env, "VTX"),
        &7,
    );
}

#[test]
fn test_name_returns_initialized_name() {
    let (env, admin, _) = setup();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin);

    assert_eq!(client.name(), String::from_str(&env, "Veritix"));
}

#[test]
fn test_symbol_returns_initialized_symbol() {
    let (env, admin, _) = setup();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin);

    assert_eq!(client.symbol(), String::from_str(&env, "VTX"));
}

#[test]
fn test_decimals_returns_initialized_decimals() {
    let (env, admin, _) = setup();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin);

    assert_eq!(client.decimals(), 7);
}

#[test]
fn test_total_supply_starts_zero_and_tracks_mint_burn() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin);

    assert_eq!(client.total_supply(), 0);

    client.mint(&admin, &user, &1000);
    assert_eq!(client.total_supply(), 1000);

    client.burn(&user, &300);
    assert_eq!(client.total_supply(), 700);
}

#[test]
fn test_balance_reflects_transfer() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let receiver = Address::generate(&env);
    initialize_client(&client, &env, &admin);

    client.mint(&admin, &user, &1000);
    assert_eq!(client.balance(&user), 1000);
    assert_eq!(client.balance(&receiver), 0);

    client.transfer(&user, &receiver, &400);
    assert_eq!(client.balance(&user), 600);
    assert_eq!(client.balance(&receiver), 400);
}

#[test]
fn test_allowance_reflects_approve_and_spend() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);
    initialize_client(&client, &env, &admin);
    client.mint(&admin, &user, &1000);

    assert_eq!(client.allowance(&user, &spender), 0);

    client.approve(&user, &spender, &500, &100);
    assert_eq!(client.allowance(&user, &spender), 500);

    client.transfer_from(&spender, &user, &spender, &200);
    assert_eq!(client.allowance(&user, &spender), 300);
}

#[test]
fn test_admin_reflects_set_admin() {
    let (env, admin, _) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin);
    let new_admin = Address::generate(&env);

    assert_eq!(client.admin(), admin);

    client.set_admin(&new_admin);
    assert_eq!(client.admin(), new_admin);
}

#[test]
fn test_is_frozen_reflects_freeze_unfreeze() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    initialize_client(&client, &env, &admin);

    assert_eq!(client.is_frozen(&user), false);

    client.freeze(&user);
    assert_eq!(client.is_frozen(&user), true);

    client.unfreeze(&user);
    assert_eq!(client.is_frozen(&user), false);
}

#[test]
fn test_escrow_count_increments_on_create() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let beneficiary = Address::generate(&env);
    initialize_client(&client, &env, &admin);
    client.mint(&admin, &user, &1000);

    assert_eq!(client.escrow_count(), 0);

    client.create_escrow(&user, &beneficiary, &500, &100);
    assert_eq!(client.escrow_count(), 1);

    client.create_escrow(&user, &beneficiary, &200, &100);
    assert_eq!(client.escrow_count(), 2);
}