use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;

#[cw_serde]
pub struct Config {
    /// Address allowed to change contract parameters.
    /// This is set to the dao address by default.
    pub owner: Addr,
}
