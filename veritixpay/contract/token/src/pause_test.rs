use soroban_sdk::{testutils::Address as _, Address, Env};
use crate::contract::VeritixToken;
use crate::pause::{is_paused, pause, require_not_paused, unpause};

fn setup_env() -> Env { let e = Env::default(); e.mock_all_auths(); e }

#[test]
fn test_pause_and_unpause_toggles_state() {
    let e = setup_env();
    let cid = e.register_contract(None, VeritixToken);
    let admin = Address::generate(&e);

    e.as_contract(&cid, || {
        crate::admin::write_admin(&e, &admin);
        assert!(!is_paused(&e));
        pause(&e, admin.clone());
        assert!(is_paused(&e));
        unpause(&e, admin.clone());
        assert!(!is_paused(&e));
    });
}
// test: add batch_test.rs comprehensive coverage

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "ContractPaused")]
fn test_require_not_paused_panics_when_paused() {
    let e = setup_env();
    let cid = e.register_contract(None, VeritixToken);
    let admin = Address::generate(&e);

    e.as_contract(&cid, || {
        crate::admin::write_admin(&e, &admin);
        pause(&e, admin.clone());
        require_not_paused(&e);
    });
}
