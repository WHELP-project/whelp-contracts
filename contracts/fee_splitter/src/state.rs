use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal};

#[cw_serde]
pub struct Config {
    /// Address allowed to change contract parameters.
    /// This is set to the dao address by default.
    pub owner: Addr,
    // List of addresses and their weights.
    // Weights must sum up to 1.0
    pub addresses: Vec<(String, Decimal)>,
}
