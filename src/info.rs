//! Contract info reporting metadata, status, admin, and package version.

use crate::admin;
use crate::control;
use soroban_sdk::{contracttype, Address, Env, String};

/// Contract diagnostic and metadata information.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractInfo {
    pub version: String,
    pub admin: Option<Address>,
    pub paused: bool,
    pub initialized: bool,
}

/// Returns the cargo package version of the contract.
pub fn version(e: &Env) -> String {
    String::from_str(e, env!("CARGO_PKG_VERSION"))
}

/// Returns current contract info containing version, admin, paused flag, and initialization state.
pub fn get_contract_info(e: &Env) -> ContractInfo {
    let initialized = admin::is_initialized(e);
    let admin_opt = if initialized {
        Some(admin::admin(e))
    } else {
        None
    };
    let paused = control::is_paused(e);

    ContractInfo {
        version: version(e),
        admin: admin_opt,
        paused,
        initialized,
    }
}
