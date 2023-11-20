use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{Addr, DepsMut, Reply, StdError, StdResult, Storage};
use cw_storage_plus::Item;
use cw_utils::MsgInstantiateContractResponse;

use super::{ContractError, PairInfo, StakeConfig};

pub type Response = cosmwasm_std::Response<CoreumMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<CoreumMsg>;

/// Stores some config options for the staking contract in-between
/// lp token instantiation and staking contract instantiation.
const TMP_STAKING_CONFIG: Item<StakeConfig> = Item::new("tmp_staking_config");

pub const LP_TOKEN_PRECISION: u32 = 6;

/// A `reply` call code ID used for staking contract instantiation sub-message.
pub const INSTANTIATE_STAKE_REPLY_ID: u64 = 2;

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
    // factory: &Addr,
    pool_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    let msg_id = msg.id;
    // parse the reply
    let res = cw_utils::parse_reply_instantiate_data(msg).map_err(|_| {
        StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
    })?;
    match msg_id {
        INSTANTIATE_STAKE_REPLY_ID => instantiate_staking_reply(deps, res, pool_info),
        _ => Err(ContractError::UnknownReply(msg_id)),
    }
}

// Sets the `pool_info`'s `staking_addr` field to the address of the newly instantiated
// staking contract, and returns a response.
pub fn instantiate_staking_reply(
    deps: &DepsMut<CoreumQueries>,
    res: MsgInstantiateContractResponse,
    pool_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    if pool_info.staking_addr != Addr::unchecked("") {
        return Err(ContractError::AddrAlreadySet("staking"));
    }

    pool_info.staking_addr = deps.api.addr_validate(&res.contract_address)?;

    Ok(Response::new().add_attribute("staking_addr", &pool_info.staking_addr))
}
