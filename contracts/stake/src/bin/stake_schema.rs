use cosmwasm_schema::write_api;
use dex::stake::InstantiateMsg;
use dex_stake::msg::{ExecuteMsg, QueryMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
    }
}
