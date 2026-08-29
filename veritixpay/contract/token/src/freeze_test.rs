use crate::freeze::{freeze_account, unfreeze_account, is_frozen};
use crate::balance::receive_balance;
use crate::balance::spend_balance;
use crate::contract::{VeritixToken, VeritixTokenClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_env(env: &Env, admin: &Address) {
    crate::admin::write_admin(env, admin);
}

#[test]
fn test_freeze_stores_true_in_persistent_storage() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VeritixToken);
    let admin = Address::generate(&env);
    let target = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        setup_env(&env, &admin);
        freeze_account(&env, admin, target.clone());
        assert_eq!(is_frozen(&env, &target), true);
    });
}

#[test]
fn test_is_frozen_returns_false_for_unfrozen_address() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VeritixToken);
    let target = Address::generate(&env);

    env.as_contract(&contract_id, || {
        assert_eq!(is_frozen(&env, &target), false);
    });
}

#[test]
fn test_unfreeze_removes_storage_entry() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VeritixToken);
    let admin = Address::generate(&env);
    let target = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        setup_env(&env, &admin);

        freeze_account(&env, admin.clone(), target.clone());
        assert_eq!(is_frozen(&env, &target), true);

        unfreeze_account(&env, admin, target.clone());
        assert_eq!(is_frozen(&env, &target), false);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "NotFrozen")]
fn test_unfreeze_not_frozen_panics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let target = Address::generate(&env);
    setup_env(&env, &admin);

    unfreeze_account(&env, admin, target);
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidFreeze")]
fn test_freeze_admin_address_panics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    setup_env(&env, &admin);

    freeze_account(&env, admin.clone(), admin);
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic]
fn test_frozen_account_cannot_spend_balance() {
    let env = Env::default();
    let target = Address::generate(&env);
    let admin = Address::generate(&env);
    setup_env(&env, &admin);

    freeze_account(&env, admin, target.clone());
    spend_balance(&env, target, 100);
}

#[test]
fn test_frozen_account_can_receive_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VeritixToken);
    let target = Address::generate(&env);
    let admin = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        setup_env(&env, &admin);
        freeze_account(&env, admin, target.clone());
        receive_balance(&env, target, 100);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not authorized: caller is not the admin")]
fn test_freeze_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VeritixToken);
    let client = VeritixTokenClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let _target = Address::generate(&env);

    client.initialize(&admin, &String::from_str(&env, "Veritix"), &String::from_str(&env, "VTX"), &7u32);

    client.freeze(&non_admin);
}