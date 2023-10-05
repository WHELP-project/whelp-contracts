use coreum_wasm_sdk::{
    assetft,
    core::{CoreumMsg, CoreumQueries},
};
use cosmwasm_std::{
    Addr, DepsMut, QuerierWrapper, Reply, Response, StdError, StdResult, Storage, SubMsg, Uint128,
};
use cw_storage_plus::Item;
use cw_utils::MsgInstantiateContractResponse;

use crate::asset::{format_lp_token_name, AssetInfoValidated};

use super::{ContractError, PairInfo, StakeConfig};

/// Stores some config options for the staking contract in-between
/// lp token instantiation and staking contract instantiation.
const TMP_STAKING_CONFIG: Item<StakeConfig> = Item::new("tmp_staking_config");

pub const LP_TOKEN_PRECISION: u32 = 6;
/// A `reply` call code ID used for token instantiation sub-message.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;
/// A `reply` call code ID used for staking contract instantiation sub-message.
const INSTANTIATE_STAKE_REPLY_ID: u64 = 2;

/// Returns a sub-message to instantiate a new LP token.
/// It uses [`INSTANTIATE_TOKEN_REPLY_ID`] as id.
pub fn create_lp_token(
    querier: &QuerierWrapper<CoreumQueries>,
    asset_infos: &[AssetInfoValidated],
) -> StdResult<SubMsg<CoreumMsg>> {
    let token_name = format_lp_token_name(asset_infos, querier)?;

    Ok(SubMsg::new(CoreumMsg::AssetFT(assetft::Msg::Issue {
        symbol: token_name,
        subunit: "uLP".to_string(),
        precision: LP_TOKEN_PRECISION,
        initial_amount: Uint128::zero(),
        description: Some("Dex LP Share token".to_string()),
        features: Some(vec![0, 1, 2]), // 0 - minting, 1 - burning, 2 - freezing
        burn_rate: Some("0".into()),
        send_commission_rate: None,
    })))
}

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
    pair_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    let msg_id = msg.id;
    // parse the reply
    let res = cw_utils::parse_reply_instantiate_data(msg).map_err(|_| {
        StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
    })?;
    match msg_id {
        INSTANTIATE_TOKEN_REPLY_ID => instantiate_lp_token_reply(deps, res, factory, pair_info),
        INSTANTIATE_STAKE_REPLY_ID => instantiate_staking_reply(deps, res, pair_info),
        _ => Err(ContractError::UnknownReply(msg_id)),
    }
}

/// Sets the `pair_info`'s `liquidity_token` field to the address of the newly instantiated
/// lp token contract, reads the temporary staking config and sends a sub-message to instantiate
/// the staking contract.
pub fn instantiate_lp_token_reply(
    deps: &DepsMut<CoreumQueries>,
    res: MsgInstantiateContractResponse,
    factory: &Addr,
    pair_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    if pair_info.liquidity_token != Addr::unchecked("") {
        return Err(ContractError::AddrAlreadySet("liquidity_token"));
    }

    pair_info.liquidity_token = deps.api.addr_validate(&res.contract_address)?;

    // now that we have the lp token, create the staking contract
    let staking_cfg = TMP_STAKING_CONFIG.load(deps.storage)?;

    Ok(Response::new()
        .add_submessage(SubMsg::reply_on_success(
            staking_cfg.into_init_msg(&deps.querier, res.contract_address, factory.to_string())?,
            INSTANTIATE_STAKE_REPLY_ID,
        ))
        .add_attribute("liquidity_token_addr", &pair_info.liquidity_token))
}

/// Sets the `pair_info`'s `staking_addr` field to the address of the newly instantiated
/// staking contract, and returns a response.
pub fn instantiate_staking_reply(
    deps: &DepsMut<CoreumQueries>,
    res: MsgInstantiateContractResponse,
    pair_info: &mut PairInfo,
) -> Result<Response, ContractError> {
    if pair_info.staking_addr != Addr::unchecked("") {
        return Err(ContractError::AddrAlreadySet("staking"));
    }

    pair_info.staking_addr = deps.api.addr_validate(&res.contract_address)?;

    Ok(Response::new().add_attribute("staking_addr", &pair_info.staking_addr))
}
