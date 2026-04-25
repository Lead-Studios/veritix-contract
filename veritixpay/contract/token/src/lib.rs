#![no_std]
#![allow(unexpected_cfgs)]

// Veritix Pay contract modules.
// Contributors: see CONTRIBUTING.md for how to get started.

pub mod admin;
pub mod allowance;
pub mod balance;
pub mod dispute;
pub mod escrow;
pub mod freeze;
pub mod metadata;
pub mod recurring;
pub mod splitter;
pub mod storage_types;
pub mod validation;

mod contract;

#[cfg(test)]
mod test;

#[cfg(test)]
mod event_test;

#[cfg(test)]
mod escrow_test;

#[cfg(test)]
mod admin_test;

#[cfg(test)]
mod splitter_test;

#[cfg(test)]
mod dispute_test;

#[cfg(test)]
mod recurring_test;

#[cfg(test)]
mod validation_test;

pub use crate::contract::VeritixToken;
