use crate::admin;
use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

/// Whether whitelist enforcement is enabled. Unset storage defaults to false.
///
/// Contract-wide configuration, so it lives in instance storage with the admin
/// and the supply counters (see `docs/ttl.md`).
pub fn is_whitelist_enabled(e: &Env) -> bool {
	e.storage()
		.instance()
		.get(&DataKey::WhitelistEnabled)
		.unwrap_or(false)
}

/// Enables whitelist enforcement for the contract.
pub fn enable_whitelist(e: &Env, admin_addr: &Address) {
	admin::check_admin(e, admin_addr);
	e.storage()
		.instance()
		.set(&DataKey::WhitelistEnabled, &true);
}

/// Disables whitelist enforcement for the contract.
pub fn disable_whitelist(e: &Env, admin_addr: &Address) {
	admin::check_admin(e, admin_addr);
	e.storage()
		.instance()
		.set(&DataKey::WhitelistEnabled, &false);
}