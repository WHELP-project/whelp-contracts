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
    // Registers a domain name for a given sender address
    RegisterName {
        // `Addr` of the user that wants to register the domain name
        sender: Addr,
        // domain name to be registered
        name: String,
    },
    // Transfers a domain name to a new owner
    TransferName {
        // `Addr` of the seller
        sender: Addr,
        // `name` is the domain name to be sold
        name: String,
        // `new_owner` is the address of the buyer
        new_owner: Addr,
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    Config {},
    #[returns(Addr)]
    NameToAddr {queried_name: String},
    #[returns(String)]
    AddrToName {queried_addr: Addr},
}
