use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{
    entry_point, to_json_binary, BankMsg, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    StdError, StdResult,
};
use cw_storage_plus::Item;
use dex::querier::query_balance;

use crate::{
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::Config,
};

/// Saves factory settings
pub const CONFIG: Item<Config> = Item::new("config");

pub type Response = cosmwasm_std::Response<CoreumMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<CoreumMsg>;

/// Contract name that is used for migration.
const _CONTRACT_NAME: &str = "fee-splitter";
/// Contract version that is used for migration.
const _CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters packed in the `msg` variable.
///
/// * **msg**  is message which contains the parameters used for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<CoreumQueries>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let is_weights_valid = msg
        .addresses
        .iter()
        .map(|&(_, weight)| weight)
        .fold(Decimal::zero(), |acc, x| acc + x)
        .le(&Decimal::from_ratio(1u32, 1u32));

    if !is_weights_valid {
        return Err(ContractError::InvalidWeights {});
    }

    let config = Config {
        addresses: msg.addresses,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: Deps<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SendTokens {
            native_denoms,
            cw20_addresses,
        } => execute_send_tokens(deps, env, info, msg, native_denoms, cw20_addresses),
    }
}

fn execute_send_tokens(
    deps: Deps<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
    native_denoms: Vec<String>,
    cw20_addresses: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    // iterate over native_denoms and send to recipients
    native_denoms.iter().for_each(|denom| {
        if let Some(amount) = query_balance(deps.querier, env.contract.address, denom) {
            let config = query_config(deps)?;
            config.addresses.iter().for_each(|(address, decimal)| {
                send_amount = amount * decimal;
                let msg = BankMsg::Send {
                    to_address: address,
                    amount,
                };
            })
        }
    });

    // iterate over cw20 address and send to recipients

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<CoreumQueries>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
    }
}

pub fn query_config(deps: Deps<CoreumQueries>) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    let resp = Config {
        owner: config.owner,
        addresses: config.addresses,
    };

    Ok(resp)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn instantiate_with_invalid_weights_should_throw_error() {
        todo!()
    }
}
