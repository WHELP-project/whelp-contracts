use cosmwasm_std::{Addr, Decimal};
use cw_storage_plus::{Map, PrimaryKey, Key};

use cosmwasm_schema::cw_serde;

pub const NAME_TO_ADDRESS: Map<DomainName, Addr> = Map::new("name_to_address");
pub const NAME_TO_OWNER: Map<DomainName, Addr> = Map::new("name_to_owner");

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub contract_name: String,
}

#[cw_serde]
pub struct DomainName {
    pub name: String,
    pub owner: Addr,
    pub price: Option<Decimal>,
}

impl<'a> PrimaryKey<'a> for &DomainName {
    type Prefix = ();

    type SubPrefix = ();

    type Suffix = Self;

    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![&self.name]
    }
}
