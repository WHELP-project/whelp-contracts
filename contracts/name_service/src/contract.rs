use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, StdResult, MessageInfo};
use cw_storage_plus::Item;
use dex::validate_name;

use crate::{
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::{Config, NAME_TO_ADDRESS, NAME_TO_OWNER, DomainName},
};

/// Saves factory settings
pub const CONFIG: Item<Config> = Item::new("config");

pub type Response = cosmwasm_std::Response<CoreumMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<CoreumMsg>;

/// Contract name that is used for migration.
const _CONTRACT_NAME: &str = "name-service";
/// Contract version that is used for migration.
const _CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters packed in the `msg` variable.
///
/// * **msg**  is message which contains the parameters used for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    validate_name!(msg.contract_name);

    let config = Config {
        admin: msg.admin.clone(),
        contract_name: msg.contract_name.clone(),
    };

    CONFIG.save(deps.storage, &config)?;
    let domain = DomainName { name: "nameservice.whelp".to_string(), owner: info.sender, price: None };
    NAME_TO_ADDRESS.save(
        deps.storage,
        domain,
        &env.contract.address.clone(),
    )?;
    NAME_TO_OWNER.save(deps.storage, msg.contract_name, &msg.admin)?;

    Ok(Response::new().add_attribute("initialized", "name service"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    unimplemented!();
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<CoreumQueries>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!();
}

pub fn query_config(deps: Deps<CoreumQueries>) -> StdResult<Config> {
    unimplemented!();
}
