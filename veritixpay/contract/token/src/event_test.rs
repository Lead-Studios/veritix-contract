use super::*;
use soroban_sdk::{testutils::Address as _, testutils::Events as _, Address, Env, String};

use crate::contract::VeritixTokenClient;

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    (env, admin, user)
}

fn create_client(env: &Env) -> VeritixTokenClient<'_> {
    let contract_id = env.register_contract(None, VeritixToken);
    VeritixTokenClient::new(env, &contract_id)
}

fn initialize_client(client: &VeritixTokenClient<'_>, env: &Env, admin: &Address, decimal: u32) {
    client.initialize(
        admin,
        &String::from_str(env, "Veritix"),
        &String::from_str(env, "VTX"),
        &decimal,
    );
}

#[test]
fn test_mint_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    let before = env.events().all().len();

    client.mint(&admin, &user, &1000i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (mint_symbol, admin), data: (to, amount)
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_burn_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    let before = env.events().all().len();

    client.burn(&user, &500i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (burn_symbol, from), data: amount
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_burn_from_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &500i128, &1000u32);
    let before = env.events().all().len();

    client.burn_from(&spender, &user, &200i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (burn_from_symbol, spender), data: (from, amount)
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_transfer_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    let before = env.events().all().len();

    client.transfer(&user, &receiver, &400i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (transfer_symbol, from), data: (to, amount)
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_transfer_from_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);
    let receiver = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    client.approve(&user, &spender, &500i128, &1000u32);
    let before = env.events().all().len();

    client.transfer_from(&spender, &user, &receiver, &300i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (xfer_from_symbol, from), data: (to, amount)
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_approve_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let spender = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    let before = env.events().all().len();

    client.approve(&user, &spender, &400i128, &1000u32);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (approve_symbol, from), data: (spender, amount)
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_clawback_event_emitted() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);
    let before = env.events().all().len();

    client.clawback(&admin, &user, &300i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 1);
    // Topics: (clawback_symbol, admin), data: (from, amount)
    assert_eq!(events.last().unwrap().1.len(), 2);
}

#[test]
fn test_all_core_ops_emit_events() {
    let (env, admin, user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let receiver = Address::generate(&env);
    let spender = Address::generate(&env);

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &user, &1000i128);

    let before = env.events().all().len();

    client.transfer(&user, &receiver, &100i128);
    client.approve(&user, &spender, &200i128, &1000u32);
    client.burn(&user, &50i128);
    client.clawback(&admin, &user, &25i128);

    let events = env.events().all();
    assert_eq!(events.len(), before + 4, "expected 4 new events for 4 operations");
}

// --- Issue #164: End-to-end ticket purchase flow tests ---

#[test]
fn test_ticket_purchase_happy_path() {
    let (env, admin, buyer) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let organiser = Address::generate(&env);
    let ticket_price = 500i128;

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &buyer, &ticket_price);

    let initial_supply = client.total_supply();
    let buyer_before = client.balance(&buyer);

    let escrow_id = client.create_escrow(&buyer, &organiser, &ticket_price, &1000u32);

    assert_eq!(client.balance(&buyer), buyer_before - ticket_price);
    assert_eq!(client.total_supply(), initial_supply);

    client.release_escrow(&organiser, &escrow_id);

    assert_eq!(client.balance(&organiser), ticket_price);
    assert_eq!(client.total_supply(), initial_supply);
}

#[test]
fn test_ticket_purchase_dispute_path() {
    let (env, admin, buyer) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    let organiser = Address::generate(&env);
    let resolver = Address::generate(&env);
    let ticket_price = 500i128;

    initialize_client(&client, &env, &admin, 7);
    client.mint(&admin, &buyer, &ticket_price);

    let initial_supply = client.total_supply();

    let escrow_id = client.create_escrow(&buyer, &organiser, &ticket_price, &1000u32);
    assert_eq!(client.balance(&buyer), 0);

    let evidence = soroban_sdk::Bytes::new(&env);
    let dispute_id = client.open_dispute(&buyer, &escrow_id, &resolver, &evidence, &1000u32, &0u32);

    client.resolve_dispute(&resolver, &dispute_id, &false);

    assert_eq!(client.balance(&buyer), ticket_price);
    assert_eq!(client.total_supply(), initial_supply);
}
