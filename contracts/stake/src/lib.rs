/// Main contract logic
pub mod contract;
/// Lazy reward distribution, mostly can be reused by other contracts
pub mod distribution;

/// custom error handler
mod error;

/// custom input output messages
pub mod msg;

/// state on the blockchain
pub mod state;

#[cfg(test)]
mod multitest;
/// some helper functions
mod utils;
pub use crate::error::ContractError;
