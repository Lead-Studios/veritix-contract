use crate::admin;
use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

/// Whether whitelist enforcement is enabled. Unset storage defaults to false.
pub fn is_whitelist_enabled(e: &Env) -> bool {
	e.storage()
		.persistent()
		.get(&DataKey::WhitelistEnabled)
		.unwrap_or(false)
}

/// Enables whitelist enforcement for the contract.
pub fn enable_whitelist(e: &Env, admin_addr: &Address) {
	admin::check_admin(e, admin_addr);
	e.storage()
		.persistent()
		.set(&DataKey::WhitelistEnabled, &true);
}

/// Disables whitelist enforcement for the contract.
pub fn disable_whitelist(e: &Env, admin_addr: &Address) {
	admin::check_admin(e, admin_addr);
	e.storage()
		.persistent()
		.set(&DataKey::WhitelistEnabled, &false);
}