use soroban_sdk::{contracttype, Env, String};

use crate::storage_types::{bump_instance, DataKey};
use crate::validation::{require_decimal_within_max, require_nonempty_string};

pub const MAX_DECIMALS: u32 = 18;

#[derive(Clone)]
#[contracttype]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimal: u32,
}

pub fn read_metadata(e: &Env) -> TokenMetadata {
    bump_instance(e);
    e.storage().instance().get(&DataKey::Metadata).unwrap()
}

pub fn write_metadata(e: &Env, metadata: TokenMetadata) {
    bump_instance(e);
    e.storage().instance().set(&DataKey::Metadata, &metadata);
}

pub fn validate_metadata(metadata: &TokenMetadata) {
    require_nonempty_string(&metadata.name, "name cannot be empty");
    require_nonempty_string(&metadata.symbol, "symbol cannot be empty");
    require_decimal_within_max(metadata.decimal, MAX_DECIMALS);
}

pub fn read_decimal(e: &Env) -> u32 {
    read_metadata(e).decimal
}

pub fn read_name(e: &Env) -> String {
    read_metadata(e).name
}

pub fn read_symbol(e: &Env) -> String {
    read_metadata(e).symbol
}

pub fn update_metadata_fields(e: &Env, name: Option<String>, symbol: Option<String>) {
    let mut metadata = read_metadata(e);
    if let Some(n) = name {
        require_nonempty_string(&n, "name cannot be empty");
        metadata.name = n;
    }
    if let Some(s) = symbol {
        require_nonempty_string(&s, "symbol cannot be empty");
        metadata.symbol = s;
    }
    write_metadata(e, metadata);
}
