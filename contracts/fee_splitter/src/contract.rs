use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{
    attr, entry_point, to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, StdResult, 
};
use cw_storage_plus::Item;
use dex::querier::{query_balance, query_token_balance};

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

    Ok(Response::new().add_attribute("initialized", "contract"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: Deps<CoreumQueries>,
    env: Env,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg.clone() {
        ExecuteMsg::SendTokens {
            native_denoms,
            cw20_addresses,
        } => execute_send_tokens(deps, env, native_denoms, cw20_addresses),
    }
}

fn execute_send_tokens(
    deps: Deps<CoreumQueries>,
    env: Env,
    native_denoms: Vec<String>,
    cw20_addresses: Vec<String>,
) -> Result<Response, ContractError> {
    let mut messages: Vec<CosmosMsg<CoreumMsg>> = vec![];
    let config = query_config(deps)?;

    native_denoms.iter().for_each(|denom| {
        if let Ok(amount) = query_balance(&deps.querier, env.clone().contract.address, denom) {
            for (address, decimal) in config.clone().addresses.into_iter() {
                let send_amount = amount * decimal;
                let msg = CosmosMsg::Bank(BankMsg::Send {
                    to_address: address.clone().to_string(),
                    amount: vec![Coin {
                        denom: denom.to_string(),
                        amount: send_amount,
                    }],
                });
                messages.push(msg);
            }
        }
    });

    // todo - dry up and down

    cw20_addresses.iter().for_each(|denom| {
        if let Ok(amount) = query_token_balance(&deps.querier, env.clone().contract.address, denom)
        {
            // config.addresses.iter().for_each(|(address, decimal)| {
            for (address, decimal) in config.clone().addresses.into_iter() {
                let send_amount = amount * decimal;
                let msg = CosmosMsg::Bank(BankMsg::Send {
                    to_address: address.clone().to_string(),
                    amount: vec![Coin {
                        denom: denom.to_string(),
                        amount: send_amount,
                    }],
                });
                messages.push(msg);
            }
        }
    });

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "withdraw")]))
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
