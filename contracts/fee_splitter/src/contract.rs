use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{
    coin, entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, StdError, StdResult, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};

use crate::{
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::{Config, CONFIG},
};

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
        .le(&Decimal::percent(100u64));

    if !is_weights_valid {
        return Err(ContractError::InvalidWeights {});
    }

    let config = Config {
        addresses: msg.addresses,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("initialized", "fee_splitter contract"))
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
    let config = CONFIG.load(deps.storage)?;

    let contract_address = env.contract.address.to_string();
    // gather balances of native tokens, either from function parameter or all
    let native_balances = native_denoms
        .into_iter()
        .map(|denom| deps.querier.query_balance(&env.contract.address, denom))
        .collect::<StdResult<Vec<Coin>>>()?;

    // gather addresses of cw20 token contract, either from arguments or configuration
    let cw20_addresses = cw20_addresses
        .into_iter()
        .map(|address| deps.api.addr_validate(&address))
        .collect::<StdResult<Vec<Addr>>>()?;

    let mut messages: Vec<CosmosMsg<CoreumMsg>> = vec![];

    for (address, weight) in config.addresses {
        let amount = native_balances
            .iter()
            .filter_map(|bcoin| {
                let amount = bcoin.amount * weight;
                if amount.is_zero() {
                    None
                } else {
                    Some(coin((bcoin.amount * weight).u128(), &bcoin.denom))
                }
            })
            .collect::<Vec<Coin>>();
        if !amount.is_empty() {
            let native_message = CosmosMsg::Bank(BankMsg::Send {
                to_address: address.to_string(),
                amount,
            });
            messages.push(native_message);
        }

        cw20_addresses
            .iter()
            // filter out if balance is zero in order to avoid empty transfer error
            .filter_map(|token| {
                match deps.querier.query_wasm_smart::<BalanceResponse>(
                    token,
                    &Cw20QueryMsg::Balance {
                        address: contract_address.clone(),
                    },
                ) {
                    Ok(r) => {
                        if !r.balance.is_zero() {
                            Some((token, r.balance))
                        } else {
                            None
                        }
                    }
                    // the only victim of current design
                    Err(_) => None,
                }
            })
            .try_for_each(|(token, balance)| {
                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: token.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: address.to_string(),
                        amount: balance * weight,
                    })?,
                    funds: vec![],
                });
                messages.push(msg);
                Ok::<(), StdError>(())
            })?;
    }
    Ok(Response::new().add_messages(messages))
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
