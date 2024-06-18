use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use dex::pool::PairInfo;

/// This structure stores the main config parameters for a constant product pool contract.
#[cw_serde]
pub struct Config {
    /// General pool information (e.g pool type)
    pub pool_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pool contract update the asset cumulative prices
    pub block_time_last: u64,
    /// The last cumulative price for asset 0
    pub price0_cumulative_last: Uint128,
    /// The last cumulative price for asset 1
    pub price1_cumulative_last: Uint128,
    /// The block time until which trading is disabled
    pub trading_starts: u64,
}

/// Stores the config struct at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// Native denom of the chain in order to sent the initial fee
pub const NATIVE_DENOM: Item<String> = Item::new("denom");
// Address which can trigger a Freeze or Unfreeze via an ExecuteMsg variant
pub const CIRCUIT_BREAKER: Item<Addr> = Item::new("circuit_breaker");
// Whether the contract is frozen or not
pub const FROZEN: Item<bool> = Item::new("frozen");

/// Stores the total amount of LP share tokens minted (workaround)
pub const LP_SHARE_AMOUNT: Item<Uint128> = Item::new("lp_share_amount");
