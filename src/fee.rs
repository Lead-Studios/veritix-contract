//! Protocol fee management and calculation.

use crate::storage_types::DataKey;
use soroban_sdk::{Address, Env};

/// Maximum allowed protocol fee basis points (10% = 1_000 bps).
pub const MAX_FEE_BPS: u32 = 1_000;

/// The configured protocol fee in basis points (defaults to 0).
pub fn get_fee_bps(e: &Env) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::ProtocolFeeBps)
        .unwrap_or(0)
}

/// The configured treasury address receiving protocol fees.
pub fn get_treasury(e: &Env) -> Option<Address> {
    e.storage().persistent().get(&DataKey::ProtocolFeeTreasury)
}

/// Sets the protocol fee basis points and treasury address. Admin-only.
///
/// # Panics
///
/// Panics when caller is not admin, or when `fee_bps` exceeds [`MAX_FEE_BPS`].
pub fn set_protocol_fee(e: &Env, admin: &Address, fee_bps: u32, treasury: &Address) {
    crate::admin::check_admin(e, admin);
    if fee_bps > MAX_FEE_BPS {
        panic!(
            "FeeBpsExceedsCap: fee_bps {} exceeds max cap of {}",
            fee_bps, MAX_FEE_BPS
        );
    }
    e.storage()
        .persistent()
        .set(&DataKey::ProtocolFeeBps, &fee_bps);
    e.storage()
        .persistent()
        .set(&DataKey::ProtocolFeeTreasury, treasury);
}

/// Calculates the protocol fee for `amount` using the configured basis points.
pub fn calculate_fee(e: &Env, amount: i128) -> i128 {
    let fee_bps = get_fee_bps(e);
    if fee_bps == 0 {
        return 0;
    }
    amount
        .checked_mul(fee_bps as i128)
        .unwrap_or_else(|| panic!("FeeOverflow: multiplication overflow in calculate_fee"))
        .checked_div(10_000)
        .unwrap_or(0)
}
