use cosmwasm_schema::cw_serde;
use cosmwasm_std::Decimal;
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    // List of addresses and their weights.
    // Weights must sum up to 1.0
    pub addresses: Vec<(String, Decimal)>,
}

pub const CONFIG: Item<Config> = Item::new("config");
