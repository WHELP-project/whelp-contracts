use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

#[cw_serde]
pub struct InstantiateMsg {
    // the admin of the contract
    pub admin: Addr,
    // name of the contract 
    pub contract_name: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Registers a domain name for a given user's address
    RegisterName {
        // domain name to be registered
        name: String,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    Config {},
}
