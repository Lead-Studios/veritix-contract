use soroban_sdk::{testutils::Address as _, Address, String};
use crate::contract_test::{setup, create_client};

// Tests for set_max_supply functionality
#[test]
fn test_set_max_supply_lower_cap_succeeds() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    
    // Initialize with max supply of 1,000,000
    client.initialize_with_max_supply(
        &admin,
        &String::from_str(&env, "Veritix"),
        &String::from_str(&env, "VTX"),
        &7u32,
        &1_000_000i128
    );
    
    // Mint some tokens to reach 500,000
    let recipient = Address::generate(&env);
    client.mint(&admin, &recipient, &500_000i128);
    
    // Lower max supply to 750,000 (which is above current supply of 500,000)
    client.set_max_supply(&admin, &750_000i128);
    
    // Verify max supply was updated
    assert_eq!(client.max_supply(), 750_000i128);
}

#[test]
#[should_panic(expected = "CannotRaiseMaxSupply")]
fn test_set_max_supply_raise_cap_panics() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    
    // Initialize with max supply of 1,000,000
    client.initialize_with_max_supply(
        &admin,
        &String::from_str(&env, "Veritix"),
        &String::from_str(&env, "VTX"),
        &7u32,
        &1_000_000i128
    );
    
    // Attempt to raise max supply to 2,000,000 - should panic
    client.set_max_supply(&admin, &2_000_000i128);
}

#[test]
#[should_panic(expected = "Cannot set max supply below current total supply")]
fn test_set_max_supply_below_current_supply_panics() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    
    // Initialize with max supply of 1,000,000
    client.initialize_with_max_supply(
        &admin,
        &String::from_str(&env, "Veritix"),
        &String::from_str(&env, "VTX"),
        &7u32,
        &1_000_000i128
    );
    
    // Mint 600,000 tokens
    let recipient = Address::generate(&env);
    client.mint(&admin, &recipient, &600_000i128);
    
    // Attempt to set max supply to 500,000 which is below current supply of 600,000 - should panic
    client.set_max_supply(&admin, &500_000i128);
}

#[test]
fn test_split_to_escrow_succeeds() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    
    client.initialize(&admin, &String::from_str(&env, "Veritix"), &String::from_str(&env, "VTX"), &7u32);
    
    let sender = Address::generate(&env);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    
    client.mint(&admin, &sender, &1000i128);
    
    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back(crate::splitter::SplitRecipient {
        address: r1.clone(),
        share_bps: 4000,
    });
    recipients.push_back(crate::splitter::SplitRecipient {
        address: r2.clone(),
        share_bps: 6000,
    });
    
    let escrow_ids = client.split_to_escrow(&sender, &recipients, &1000i128, &2000u32);
    assert_eq!(escrow_ids.len(), 2);
    
    let escrow1 = client.get_escrow(&escrow_ids.get(0).unwrap());
    assert_eq!(escrow1.depositor, sender);
    assert_eq!(escrow1.beneficiary, r1);
    assert_eq!(escrow1.amount, 400);
    
    let escrow2 = client.get_escrow(&escrow_ids.get(1).unwrap());
    assert_eq!(escrow2.depositor, sender);
    assert_eq!(escrow2.beneficiary, r2);
    assert_eq!(escrow2.amount, 600);
}

#[test]
#[should_panic(expected = "TooManyRecipients")]
fn test_split_to_escrow_too_many_recipients_panics() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    
    client.initialize(&admin, &String::from_str(&env, "Veritix"), &String::from_str(&env, "VTX"), &7u32);
    
    let sender = Address::generate(&env);
    client.mint(&admin, &sender, &1000i128);
    
    let mut recipients = soroban_sdk::Vec::new(&env);
    for _ in 0..21 {
        recipients.push_back(crate::splitter::SplitRecipient {
            address: Address::generate(&env),
            share_bps: 10,
        });
    }
    
    client.split_to_escrow(&sender, &recipients, &1000i128, &2000u32);
}

#[test]
#[should_panic(expected = "InvalidShares")]
fn test_split_to_escrow_invalid_shares_panics() {
    let (env, admin, _user) = setup();
    env.mock_all_auths();
    let client = create_client(&env);
    
    client.initialize(&admin, &String::from_str(&env, "Veritix"), &String::from_str(&env, "VTX"), &7u32);
    
    let sender = Address::generate(&env);
    let r1 = Address::generate(&env);
    
    client.mint(&admin, &sender, &1000i128);
    
    let mut recipients = soroban_sdk::Vec::new(&env);
    recipients.push_back(crate::splitter::SplitRecipient {
        address: r1,
        share_bps: 9999,
    });
    
    client.split_to_escrow(&sender, &recipients, &1000i128, &2000u32);
}