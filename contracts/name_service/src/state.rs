use cosmwasm_std::Addr;
use cw_storage_plus::Map;

use cosmwasm_schema::cw_serde;

pub const NAME_TO_ADDRESS: Map<String, Addr> = Map::new("name_to_address");
pub const NAME_TO_OWNER: Map<String, Addr> = Map::new("name_to_owner");

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub contract_name: String,
}
