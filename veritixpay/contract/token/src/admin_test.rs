#[cfg(test)]
mod admin_test {
    use soroban_sdk::{testutils::Address as _, testutils::Events as _, Address, Env, String};

    use crate::admin::{has_admin, transfer_admin, write_admin};
    use crate::contract::VeritixToken;
    use crate::contract::VeritixTokenClient;

    fn setup_env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    fn create_initialized_client(env: &Env) -> (Address, VeritixTokenClient<'_>) {
        let contract_id = env.register_contract(None, VeritixToken);
        let client = VeritixTokenClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(
            &admin,
            &String::from_str(env, "Veritix"),
            &String::from_str(env, "VTX"),
            &7u32,
        );
        (admin, client)
    }

    // --- test_initialize_sets_admin ---

    #[test]
    fn test_initialize_sets_admin() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        assert_eq!(client.admin(), admin);
    }

    // --- test_has_admin_false_before_initialize ---

    #[test]
    fn test_has_admin_false_before_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, VeritixToken);
        env.as_contract(&contract_id, || {
            assert!(!has_admin(&env));
        });
    }

    // --- test_transfer_admin_updates_stored_admin ---

    #[test]
    fn test_transfer_admin_updates_stored_admin() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        let new_admin = Address::generate(&env);

        client.set_admin(&new_admin);

        assert_eq!(client.admin(), new_admin);
        assert_ne!(client.admin(), admin);
    }

    // --- test_transfer_admin_unauthorized_panics ---

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic]
    fn test_transfer_admin_unauthorized_panics() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        write_admin(&env, &admin);

        // No mock auths — transfer_admin requires the current admin to authorize
        env.set_auths(&[]);
        transfer_admin(&env, new_admin);
    }

    // --- test_transfer_admin_emits_event ---

    #[test]
    fn test_transfer_admin_emits_event() {
        let env = setup_env();
        let (_admin, client) = create_initialized_client(&env);
        let new_admin = Address::generate(&env);

        // Clear any initialization events
        let _ = env.events().all();

        client.set_admin(&new_admin);

        let events = env.events().all();
        assert!(!events.is_empty(), "expected at least one event after set_admin");

        // The admin_set event topics: (symbol_short!("admin_set"), current_admin)
        // data: new_admin
        let event = events.first().unwrap();
        assert_eq!(event.1.len(), 2);
    }

    // --- test_check_admin_wrong_address_panics ---

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic]
    fn test_check_admin_wrong_address_panics() {
        let env = setup_env();
        let contract_id = env.register_contract(None, VeritixToken);
        let admin = Address::generate(&env);
        let impostor = Address::generate(&env);

        env.as_contract(&contract_id, || {
            write_admin(&env, &admin);
            // check_admin with a non-admin address should panic
            crate::admin::check_admin(&env, &impostor);
        });
    }

    // --- test_transfer_admin (basic rotation) ---

    #[test]
    fn test_transfer_admin() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        let new_admin = Address::generate(&env);

        client.set_admin(&new_admin);

        assert_eq!(client.admin(), new_admin);
        assert_ne!(client.admin(), admin);
    }

    // --- test_transfer_admin_to_same_address ---

    #[test]
    fn test_transfer_admin_to_same_address() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);

        client.set_admin(&admin);
        assert_eq!(client.admin(), admin);
    }

    #[test]
    fn test_admin_info_tracks_admin_rotation() {
        let env = setup_env();
        let (_admin, client) = create_initialized_client(&env);
        let new_admin = Address::generate(&env);
        let before = client.admin_info();
        assert_eq!(before.paused, false);
        client.set_admin(&new_admin);
        let after = client.admin_info();
        assert_eq!(after.admin, new_admin);
        assert_eq!(after.paused, false);
    }

    #[test]
    fn test_freeze_batch_and_unfreeze_batch() {
        let env = setup_env();
        let (_admin, client) = create_initialized_client(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let mut targets = soroban_sdk::Vec::new(&env);
        targets.push_back(a.clone());
        targets.push_back(b.clone());
        client.freeze_batch(&client.admin(), &targets);
        assert!(client.is_frozen(&a));
        assert!(client.is_frozen(&b));
        client.unfreeze_batch(&client.admin(), &targets);
        assert!(!client.is_frozen(&a));
        assert!(!client.is_frozen(&b));
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    fn test_clawback_batch_reduces_balances_and_supply() {
        let env = setup_env();
        let (_admin, client) = create_initialized_client(&env);
        let admin = client.admin();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        client.mint(&admin, &a, &1000);
        client.mint(&admin, &b, &1000);
        let mut targets = soroban_sdk::Vec::new(&env);
        targets.push_back((a.clone(), 200));
        targets.push_back((b.clone(), 300));
        let before_supply = client.total_supply();
        client.clawback_batch(&admin, &targets);
        assert_eq!(client.balance(&a), 800);
        assert_eq!(client.balance(&b), 700);
        assert_eq!(client.total_supply(), before_supply - 500);
    }

    #[test]
    fn test_clawback_no_cosigner_single_admin_auth_sufficient() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        let victim = Address::generate(&env);

        client.mint(&admin, &victim, &1_000i128);
        // No cosigner set — single admin auth must work.
        client.clawback(&admin, &victim, &200i128);

        assert_eq!(client.balance(&victim), 800i128);
        assert_eq!(client.total_supply(), 800i128);
    }

    #[test]
    fn test_set_clawback_cosigner_requires_both_auths() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        let cosigner = Address::generate(&env);
        let victim = Address::generate(&env);

        client.mint(&admin, &victim, &1_000i128);
        client.set_clawback_cosigner(&admin, &cosigner);

        // With cosigner set, mock_all_auths covers both — call must succeed.
        client.clawback(&admin, &victim, &400i128);
        assert_eq!(client.balance(&victim), 600i128);
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic]
    fn test_clawback_missing_cosigner_auth_panics() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        let cosigner = Address::generate(&env);
        let victim = Address::generate(&env);

        client.mint(&admin, &victim, &1_000i128);
        client.set_clawback_cosigner(&admin, &cosigner);

        // Remove cosigner's auth — clawback must now panic.
        env.set_auths(&[]);
        client.clawback(&admin, &victim, &100i128);
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    fn test_clawback_batch_with_cosigner() {
        let env = setup_env();
        let (admin, client) = create_initialized_client(&env);
        let cosigner = Address::generate(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);

        client.mint(&admin, &a, &500i128);
        client.mint(&admin, &b, &500i128);
        client.set_clawback_cosigner(&admin, &cosigner);

        let mut targets = soroban_sdk::Vec::new(&env);
        targets.push_back((a.clone(), 100));
        targets.push_back((b.clone(), 200));

        // Both admin and cosigner are mock-authed.
        client.clawback_batch(&admin, &targets);
        assert_eq!(client.balance(&a), 400i128);
        assert_eq!(client.balance(&b), 300i128);
    }

    // --- Issue #287: Old admin cannot act after rotation ---

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not authorized: caller is not the admin")]
    fn test_old_admin_cannot_mint_after_rotation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, VeritixToken);
        let admin_a = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            write_admin(&env, &admin_a);
        });

        env.mock_all_auths();
        let client = VeritixTokenClient::new(&env, &contract_id);

        // Transfer admin from admin_a to admin_b
        client.set_admin(&admin_b);

        // Clear auths and try minting as admin_a (old admin) - should panic
        env.set_auths(&[]);
        client.mint(&admin_a, &user, &100i128);
    }

    #[test]
    #[cfg_attr(windows, ignore)]
    #[should_panic(expected = "not authorized: caller is not the admin")]
    fn test_old_admin_cannot_freeze_after_rotation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, VeritixToken);
        let admin_a = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            write_admin(&env, &admin_a);
        });

        env.mock_all_auths();
        let client = VeritixTokenClient::new(&env, &contract_id);

        // Transfer admin from admin_a to admin_b
        client.set_admin(&admin_b);

        // Clear auths and try freezing as admin_a (old admin) - should panic
        env.set_auths(&[]);
        client.freeze(&user);
    }

    #[test]
    fn test_new_admin_can_mint_after_rotation() {
        let env = setup_env();
        let (_admin_a, client) = create_initialized_client(&env);
        let admin_b = Address::generate(&env);
        let user = Address::generate(&env);

        // Transfer admin from admin_a to admin_b
        client.set_admin(&admin_b);

        // admin_b should be able to mint successfully
        client.mint(&admin_b, &user, &100i128);
        assert_eq!(client.balance(&user), 100i128);
    }
}
