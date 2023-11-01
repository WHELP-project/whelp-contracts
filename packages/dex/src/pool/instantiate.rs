use coreum_wasm_sdk::{
    core::CoreumQueries,
};
use cosmwasm_std::{
    Addr, DepsMut, Reply, Response, StdError, StdResult, Storage,
};
use cw_storage_plus::Item;
use cw_utils::MsgExecuteContractResponse;

use super::{ContractError, PairInfo, StakeConfig};

/// Stores some config options for the staking contract in-between
/// lp token instantiation and staking contract instantiation.
const TMP_STAKING_CONFIG: Item<StakeConfig> = Item::new("tmp_staking_config");

pub const LP_TOKEN_PRECISION: u32 = 6;
/// A `reply` call code ID used for token instantiation sub-message.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;
/// A `reply` call code ID used for staking contract instantiation sub-message.
const _INSTANTIATE_STAKE_REPLY_ID: u64 = 2;

/// Saves this `stake_config` to the storage temporarily
/// until the reply for creating the lp token arrives.
pub fn save_tmp_staking_config(
    storage: &mut dyn Storage,
    stake_config: &StakeConfig,
) -> StdResult<()> {
    TMP_STAKING_CONFIG.save(storage, stake_config)
}

/// Handles the replies from the lp token and staking contract instantiation sub-messages.
pub fn handle_reply(
    deps: &DepsMut<CoreumQueries>,
    msg: Reply,
    factory: &Addr,
    pool_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    let msg_id = msg.id;
    // parse the reply
    let res = cw_utils::parse_reply_execute_data(msg).map_err(|_| {
        StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
    })?;
    match msg_id {
        INSTANTIATE_TOKEN_REPLY_ID => instantiate_lp_token_reply(deps, res, factory, pool_info),
        // INSTANTIATE_STAKE_REPLY_ID => instantiate_staking_reply(deps, res, pool_info),
        _ => Err(ContractError::UnknownReply(msg_id)),
    }
}

/// Sets the `pool_info`'s `liquidity_token` field to the address of the newly instantiated
/// lp token contract, reads the temporary staking config and sends a sub-message to instantiate
/// the staking contract.
pub fn instantiate_lp_token_reply(
    deps: &DepsMut<CoreumQueries>,
    res: MsgExecuteContractResponse,
    _factory: &Addr,
    pool_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    if pool_info.liquidity_token != Addr::unchecked("") {
        return Err(ContractError::AddrAlreadySet("liquidity_token"));
    }

    // pool_info.liquidity_token = deps.api.addr_validate(&res.contract_address)?;

    // now that we have the lp token, create the staking contract
    // let staking_cfg = TMP_STAKING_CONFIG.load(deps.storage)?;

    Ok(Response::new()
        // .add_submessage(SubMsg::new(
        //     staking_cfg.into_init_msg(&deps.querier, res.contract_address, factory.to_string())?,
        // ))
        .add_attribute("liquidity_token_addr", &pool_info.liquidity_token))
}

// Sets the `pool_info`'s `staking_addr` field to the address of the newly instantiated
// staking contract, and returns a response.
// pub fn instantiate_staking_reply(
//     deps: &DepsMut<CoreumQueries>,
//     res: MsgInstantiateContractResponse,
//     pool_info: &mut PairInfo,
// ) -> Result<Response, ContractError> {
//     if pool_info.staking_addr != Addr::unchecked("") {
//         return Err(ContractError::AddrAlreadySet("staking"));
//     }
//
//     pool_info.staking_addr = deps.api.addr_validate(&res.contract_address)?;
//
//     Ok(Response::new().add_attribute("staking_addr", &pool_info.staking_addr))
// }
