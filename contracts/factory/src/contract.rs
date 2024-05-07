use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{
    attr, entry_point, from_json, to_json_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Order, Reply, ReplyOn, StdError, StdResult, WasmMsg,
};
use cw2::{ensure_from_older_version, set_contract_version};
use cw20::Cw20ReceiveMsg;

use dex::{
    asset::{addr_opt_validate, Asset, AssetInfo},
    common::{claim_ownership, drop_ownership_proposal, propose_new_owner, validate_addresses},
    factory::{
        ConfigResponse, DistributionFlow, ExecuteMsg, FeeInfoResponse, InstantiateMsg, MigrateMsg,
        PartialDefaultStakeConfig, PartialStakeConfig, PoolConfig, PoolType, PoolsResponse,
        QueryMsg, ReceiveMsg, ROUTE,
    },
    fee_config::FeeConfig,
    pool::{ExecuteMsg as PoolExecuteMsg, InstantiateMsg as PoolInstantiateMsg, PairInfo},
    stake::UnbondingPeriod,
};
use dex_stake::msg::ExecuteMsg as StakeExecuteMsg;

use crate::{
    error::ContractError,
    querier::query_pair_info,
    state::{
        check_asset_infos, pair_key, read_pairs, Config, TmpPoolInfo, CONFIG, OWNERSHIP_PROPOSAL,
        PAIRS, PAIRS_TO_MIGRATE, PAIR_CONFIGS, PERMISSIONLESS_DEPOSIT_REQUIREMENT, POOL_TYPES,
        STAKING_ADDRESSES, TMP_PAIR_INFO,
    },
};

use itertools::Itertools;
use std::collections::HashSet;

pub type Response = cosmwasm_std::Response<CoreumMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<CoreumMsg>;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "dex-factory";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// A `reply` call code ID used in a sub-message.
const INSTANTIATE_PAIR_REPLY_ID: u64 = 1;

const SECONDS_PER_DAY: u64 = 60 * 60 * 24;
/// The maximum amount of seconds that the trading can be delayed when the contract is instantiated.
const MAX_TRADING_STARTS_DELAY: u64 = 60 * SECONDS_PER_DAY;

/// Creates a new contract with the specified parameters packed in the `msg` variable.
///
/// * **msg**  is message which contains the parameters used for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if msg.max_referral_commission > Decimal::one() {
        return Err(ContractError::InvalidReferralCommission(
            msg.max_referral_commission,
        ));
    }

    if let Some(trading_starts) = msg.trading_starts {
        let block_time = env.block.time.seconds();
        if trading_starts < block_time || trading_starts > block_time + MAX_TRADING_STARTS_DELAY {
            return Err(ContractError::InvalidTradingStart {});
        }
    }

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        fee_address: addr_opt_validate(deps.api, &msg.fee_address)?,
        max_referral_commission: msg.max_referral_commission,
        default_stake_config: msg.default_stake_config,
        only_owner_can_create_pools: false,
        trading_starts: msg.trading_starts,
    };

    let config_set: HashSet<String> = msg
        .pool_configs
        .iter()
        .map(|pc| pc.pool_type.to_string())
        .collect();

    if config_set.len() != msg.pool_configs.len() {
        return Err(ContractError::PoolConfigDuplicate {});
    }

    for pc in msg.pool_configs.iter() {
        // Validate total and protocol fee bps
        if !pc.fee_config.valid_fee_bps() {
            return Err(ContractError::PoolConfigInvalidFeeBps {});
        }
        PAIR_CONFIGS.save(deps.storage, pc.pool_type.to_string(), pc)?;
    }
    PERMISSIONLESS_DEPOSIT_REQUIREMENT.save(deps.storage, &msg.permissionless_fee_requirement)?;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// Data structure used to update general contract parameters.
pub struct UpdateConfig {
    /// Contract address to send governance fees to (the Protocol)
    fee_address: Option<String>,
    /// Whether only the owner or anyone can create new pairs
    only_owner_can_create_pools: Option<bool>,
    /// The default configuration for the staking contracts of new pairs
    default_stake_config: Option<PartialDefaultStakeConfig>,
}

/// Exposes all the execute functions available in the contract.
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Variants
/// * **ExecuteMsg::UpdateConfig {
///             fee_address,
///         }** Updates general contract parameters.
///
/// * **ExecuteMsg::UpdatePoolConfig { config }** Updates a pair type
/// * configuration or creates a new pair type if a [`Custom`] name is used (which hasn't been used before).
///
/// * **ExecuteMsg::CreatePool {
///             pool_type,
///             asset_infos,
///             init_params,
///         }** Creates a new pair with the specified input parameters.
///
/// * **ExecuteMsg::Deregister { asset_infos }** Removes an existing pair from the factory.
/// * The asset information is for the assets that are traded in the pair.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
///
/// * **ExecuteMsg::MarkAsMigrated {}** Mark pairs as migrated.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            fee_address,
            only_owner_can_create_pools,
            default_stake_config,
        } => execute_update_config(
            deps,
            info,
            UpdateConfig {
                fee_address,
                only_owner_can_create_pools,
                default_stake_config,
            },
        ),
        ExecuteMsg::UpdatePoolFees {
            asset_infos,
            fee_config,
        } => execute_update_pair_fees(deps, info, asset_infos, fee_config),
        ExecuteMsg::UpdatePoolConfig { config } => execute_update_pair_config(deps, info, config),
        ExecuteMsg::CreatePool {
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
        } => execute_create_pair(
            deps,
            info,
            env,
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
            Vec::new(),
        ),
        ExecuteMsg::Deregister { asset_infos } => {
            deregister_pool_and_staking(deps, info, asset_infos)
        }
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            let pairs = PAIRS
                .range(deps.storage, None, None, Order::Ascending)
                .map(|pair| -> StdResult<Addr> { Ok(pair?.1) })
                .collect::<StdResult<Vec<_>>>()?;

            PAIRS_TO_MIGRATE.save(deps.storage, &pairs)?;

            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::MarkAsMigrated { pools } => execute_mark_pairs_as_migrated(deps, info, pools),
        ExecuteMsg::CreatePoolAndDistributionFlows {
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
            distribution_flows,
        } => execute_create_pair(
            deps,
            info,
            env,
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
            distribution_flows,
        ),
        ExecuteMsg::CreateDistributionFlow {
            asset_infos,
            asset,
            rewards,
        } => execute_create_distribution_flow(deps, env, info, asset_infos, asset, rewards),
        ExecuteMsg::Receive(msg) => receive_cw20_message(deps, env, info, msg),
    }
}

fn receive_cw20_message(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let required_deposit = PERMISSIONLESS_DEPOSIT_REQUIREMENT
        .load(deps.storage)
        .map_err(|_| ContractError::DepositNotSet {})?;
    let deposit = Asset {
        info: AssetInfo::Cw20Token(info.sender.to_string()),
        amount: msg.amount,
    };

    if required_deposit != deposit {
        return Err(ContractError::DepositRequired(
            required_deposit.amount,
            required_deposit.info.to_string(),
        ));
    }

    match from_json(&msg.msg)? {
        ReceiveMsg::CreatePool {
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
        } => execute_create_pair(
            deps,
            info,
            env,
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
            Vec::new(),
        ),
        ReceiveMsg::CreatePoolAndDistributionFlows {
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
            distribution_flows,
        } => execute_create_pair(
            deps,
            info,
            env,
            pool_type,
            asset_infos,
            init_params,
            total_fee_bps,
            staking_config,
            distribution_flows,
        ),
    }
}

fn execute_update_pair_fees(
    deps: DepsMut<CoreumQueries>,
    info: MessageInfo,
    asset_infos: Vec<AssetInfo>,
    fee_config: FeeConfig,
) -> Result<Response, ContractError> {
    // check permissions
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // validate
    let asset_infos = check_asset_infos(deps.api, &asset_infos)?;

    // get pair address
    let pair = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;

    // send update message to pair
    Ok(Response::default().add_message(WasmMsg::Execute {
        contract_addr: pair.to_string(),
        msg: to_json_binary(&PoolExecuteMsg::UpdateFees { fee_config })?,
        funds: Vec::new(),
    }))
}

/// Forwards distribution flow creation to the correct LP token staking contract.
///
/// * **asset_infos** is the pair of assets whose LP token staking contract should get the new distribution flow.
///
/// * **asset** is the asset that should be distributed.
///
/// * **rewards** contains the reward multiplier per unbonding period.
///
/// * **reward_duration** is the duration of scheduled distributions.
///
/// ## Executor
/// Only the owner can execute this.
fn execute_create_distribution_flow(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    asset_infos: Vec<AssetInfo>,
    asset: AssetInfo,
    rewards: Vec<(UnbondingPeriod, Decimal)>,
) -> Result<Response, ContractError> {
    // check permission
    if info.sender != CONFIG.load(deps.storage)?.owner {
        return Err(ContractError::Unauthorized {});
    }

    let asset_infos = check_asset_infos(deps.api, &asset_infos)?;
    let pair = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    let staking = query_pair_info(&deps.querier, pair)?.staking_addr;
    Ok(
        Response::new().add_submessage(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking.to_string(),
            msg: to_json_binary(&StakeExecuteMsg::CreateDistributionFlow {
                manager: env.contract.address.to_string(), // use factory as manager for now
                asset,
                rewards,
            })?,
            funds: vec![],
        }))),
    )
}

/// Updates general contract settings.
///
/// * **param** is an object of type [`UpdateConfig`] that contains the parameters to update.
///
/// ## Executor
/// Only the owner can execute this.
pub fn execute_update_config(
    deps: DepsMut<CoreumQueries>,
    info: MessageInfo,
    param: UpdateConfig,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(fee_address) = param.fee_address {
        // Validate address format
        config.fee_address = Some(deps.api.addr_validate(&fee_address)?);
    }

    if let Some(only_owner) = param.only_owner_can_create_pools {
        config.only_owner_can_create_pools = only_owner;
    }

    if let Some(default_stake_config) = param.default_stake_config {
        config.default_stake_config.update(default_stake_config);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// Updates a pair type's configuration.
///
/// * **pair_config** is an object of type [`PoolConfig`] that contains the pair type information to update.
///
/// ## Executor
/// Only the owner can execute this.
pub fn execute_update_pair_config(
    deps: DepsMut<CoreumQueries>,
    info: MessageInfo,
    pair_config: PoolConfig,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Validate total and protocol fee bps
    if !pair_config.fee_config.valid_fee_bps() {
        return Err(ContractError::PoolConfigInvalidFeeBps {});
    }

    PAIR_CONFIGS.save(
        deps.storage,
        pair_config.pool_type.to_string(),
        &pair_config,
    )?;

    Ok(Response::new().add_attribute("action", "update_pair_config"))
}

/// Creates a new pair of `pool_type` with the assets specified in `asset_infos`.
///
/// * **pool_type** is the pair type of the newly created pair.
///
/// * **asset_infos** is a vector with assets for which we create a pair.
///
/// * **init_params** These are packed params used for custom pair types that need extra data to be instantiated.
///
/// * **staking_config** is the configuration for the staking contract. Overrides the default staking config.
///
/// * **distribution_flows** is a vector of distribution flows to be created for the pair's staking contract.
#[allow(clippy::too_many_arguments)]
pub fn execute_create_pair(
    deps: DepsMut<CoreumQueries>,
    info: MessageInfo,
    env: Env,
    pool_type: PoolType,
    asset_infos: Vec<AssetInfo>,
    init_params: Option<Binary>,
    total_fee_bps: Option<u16>,
    staking_config: PartialStakeConfig,
    distribution_flows: Vec<DistributionFlow>,
) -> Result<Response, ContractError> {
    let asset_infos = check_asset_infos(deps.api, &asset_infos)?;

    let config = CONFIG.load(deps.storage)?;

    if config.only_owner_can_create_pools && info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if !config.only_owner_can_create_pools && !permissionless_fee_sent(&deps, info) {
        return Err(ContractError::PermissionlessRequiresDeposit {});
    }

    if PAIRS.has(deps.storage, &pair_key(&asset_infos)) {
        return Err(ContractError::PoolWasCreated {});
    }

    // Get pair type from config
    let pair_config = PAIR_CONFIGS
        .load(deps.storage, pool_type.to_string())
        .map_err(|_| ContractError::PoolConfigNotFound {})?;

    // Check if pair config is disabled
    if pair_config.is_disabled {
        return Err(ContractError::PoolConfigDisabled {});
    }

    let pair_key = pair_key(&asset_infos);
    TMP_PAIR_INFO.save(
        deps.storage,
        &TmpPoolInfo {
            pair_key,
            asset_infos: asset_infos.clone(),
            distribution_flows,
        },
    )?;

    let sub_msg: Vec<SubMsg> = vec![SubMsg {
        id: INSTANTIATE_PAIR_REPLY_ID,
        msg: WasmMsg::Instantiate {
            admin: Some(config.owner.to_string()),
            code_id: pair_config.code_id,
            msg: to_json_binary(&PoolInstantiateMsg {
                asset_infos: asset_infos.iter().cloned().map(Into::into).collect(),
                factory_addr: env.contract.address.to_string(),
                init_params,
                staking_config: config
                    .default_stake_config
                    .combine_with(staking_config)
                    .to_stake_config(),
                trading_starts: config
                    .trading_starts
                    .unwrap_or_else(|| env.block.time.seconds()),
                fee_config: FeeConfig {
                    total_fee_bps: total_fee_bps.unwrap_or(pair_config.fee_config.total_fee_bps),
                    protocol_fee_bps: pair_config.fee_config.protocol_fee_bps,
                },
                circuit_breaker: None,
            })?,
            funds: vec![],
            label: "Dex pair".to_string(),
        }
        .into(),
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }];

    Ok(Response::new()
        .add_submessages(sub_msg)
        .add_attributes(vec![
            attr("action", "create_pair"),
            attr("pair", asset_infos.iter().join("-")),
        ]))
}

/// Marks specified pairs as migrated to the new admin.
///
/// * **pairs** is a vector of pairs which should be marked as transferred.
fn execute_mark_pairs_as_migrated(
    deps: DepsMut<CoreumQueries>,
    info: MessageInfo,
    pairs: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let pairs = validate_addresses(deps.api, &pairs)?;

    let not_migrated: Vec<Addr> = PAIRS_TO_MIGRATE
        .load(deps.storage)?
        .into_iter()
        .filter(|addr| !pairs.contains(addr))
        .collect();

    PAIRS_TO_MIGRATE.save(deps.storage, &not_migrated)?;
    Ok(Response::new().add_attribute("action", "execute_mark_pairs_as_migrated"))
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    // parse the reply
    let res = cw_utils::parse_reply_instantiate_data(msg).map_err(|_| {
        StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
    })?;

    reply::instantiate_pair(deps, env, res)
}

fn permissionless_fee_sent(deps: &DepsMut<CoreumQueries>, info: MessageInfo) -> bool {
    let deposit_required = PERMISSIONLESS_DEPOSIT_REQUIREMENT
        .load(deps.storage)
        .map_err(|_| ContractError::DepositNotSet {})
        .unwrap();

    info.funds.iter().any(|coin| {
        coin.amount >= deposit_required.amount && coin.denom == deposit_required.info.to_string()
    })
}

pub mod reply {
    use cosmwasm_std::wasm_execute;
    use cw_utils::MsgInstantiateContractResponse;

    use crate::state::STAKING_ADDRESSES;

    use super::*;

    pub fn instantiate_pair(
        deps: DepsMut<CoreumQueries>,
        env: Env,
        res: MsgInstantiateContractResponse,
    ) -> Result<Response, ContractError> {
        let tmp = TMP_PAIR_INFO.load(deps.storage)?;
        if PAIRS.has(deps.storage, &tmp.pair_key) {
            return Err(ContractError::PoolWasRegistered {});
        }

        let pair_contract = deps.api.addr_validate(&res.contract_address)?;

        PAIRS.save(deps.storage, &tmp.pair_key, &pair_contract)?;

        for asset_info in &tmp.asset_infos {
            for asset_info_2 in &tmp.asset_infos {
                if asset_info != asset_info_2 {
                    ROUTE.update::<_, StdError>(
                        deps.storage,
                        (asset_info.to_string(), asset_info_2.to_string()),
                        |maybe_contracts| {
                            if let Some(mut contracts) = maybe_contracts {
                                contracts.push(pair_contract.clone());
                                Ok(contracts)
                            } else {
                                Ok(vec![pair_contract.clone()])
                            }
                        },
                    )?;
                }
            }
        }

        // keep track of staking address
        let pair_info = query_pair_info(&deps.querier, &pair_contract)?;
        STAKING_ADDRESSES.save(deps.storage, &pair_info.staking_addr, &())?;

        Ok(Response::new()
            // create distribution flows
            .add_submessages(tmp.distribution_flows.into_iter().map(|flow| {
                SubMsg::new(
                    wasm_execute(
                        &pair_info.staking_addr,
                        &dex_stake::msg::ExecuteMsg::CreateDistributionFlow {
                            manager: env.contract.address.to_string(),
                            asset: flow.asset,
                            rewards: flow.rewards,
                        },
                        vec![],
                    )
                    .unwrap(),
                )
            }))
            .add_attributes(vec![
                attr("action", "register"),
                attr("pair_contract_addr", pair_contract),
            ]))
    }
}

/// Removes an existing pair from the factory.
///
/// * **asset_infos** is a vector with assets for which we deregister the pair.
/// The LP Staking Contract will also be deregistered and does not need to be provided.
///
/// ## Executor
/// Only the owner can execute this.
pub fn deregister_pool_and_staking(
    deps: DepsMut<CoreumQueries>,
    info: MessageInfo,
    asset_infos: Vec<AssetInfo>,
) -> Result<Response, ContractError> {
    let asset_infos: Result<Vec<_>, _> = asset_infos
        .into_iter()
        .map(|a| a.validate(deps.api))
        .collect();
    let asset_infos = asset_infos?;

    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let pair_addr = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    PAIRS.remove(deps.storage, &pair_key(&asset_infos));
    // keep track of staking address
    let pair_info = query_pair_info(&deps.querier, &pair_addr)?;
    STAKING_ADDRESSES.remove(deps.storage, &pair_info.staking_addr);

    for asset_info1 in &asset_infos {
        for asset_info2 in &asset_infos {
            if asset_info1 != asset_info2 {
                ROUTE.update::<_, StdError>(
                    deps.storage,
                    (asset_info1.to_string(), asset_info2.to_string()),
                    |pairs| {
                        Ok(pairs
                            .unwrap_or_default()
                            .iter()
                            .filter(|&pair| pair != pair_addr)
                            .cloned()
                            .collect::<Vec<_>>())
                    },
                )?;
            }
        }
    }

    Ok(Response::new().add_attributes(vec![
        attr("action", "deregister"),
        attr("pair_contract_addr", pair_addr),
    ]))
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns general contract parameters using a custom [`ConfigResponse`] structure.
///
/// * **QueryMsg::Pool { asset_infos }** Returns a [`PoolInfo`] object with information about a specific Dex pair.
///
/// * **QueryMsg::Pools { start_after, limit }** Returns an array that contains items of type [`PoolInfo`].
/// This returns information about multiple Dex pairs
///
/// * **QueryMsg::FeeInfo { pool_type }** Returns the fee structure (total and protocol fees) for a specific pair type.
///
/// * **QueryMsg::BlacklistedPoolTypes {}** Returns a vector that contains blacklisted pair types (pair types that cannot get ASTRO emissions).
///
/// * **QueryMsg::PoolsToMigrate {}** Returns a vector that contains pair addresses that are not migrated.
///
/// * **QueryMsg::PoolsType { address }** Returns the pool type of the specified address.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<CoreumQueries>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Pool { asset_infos } => to_json_binary(&query_pair(deps, asset_infos)?),
        QueryMsg::Pools { start_after, limit } => {
            to_json_binary(&query_pairs(deps, start_after, limit)?)
        }
        QueryMsg::FeeInfo { pool_type } => to_json_binary(&query_fee_info(deps, pool_type)?),
        QueryMsg::BlacklistedPoolTypes {} => to_json_binary(&query_blacklisted_pool_types(deps)?),
        QueryMsg::PoolsToMigrate {} => {
            to_json_binary(&PAIRS_TO_MIGRATE.may_load(deps.storage)?.unwrap_or_default())
        }
        QueryMsg::ValidateStakingAddress { address } => {
            to_json_binary(&STAKING_ADDRESSES.has(deps.storage, &deps.api.addr_validate(&address)?))
        }
        QueryMsg::PoolsType { address } => to_json_binary(&query_pool_type(deps, address)?),
    }
}

/// Returns a vector that contains blacklisted pair types
pub fn query_blacklisted_pool_types(deps: Deps<CoreumQueries>) -> StdResult<Vec<PoolType>> {
    PAIR_CONFIGS
        .range(deps.storage, None, None, Order::Ascending)
        .filter_map(|result| match result {
            Ok(v) => {
                if v.1.is_disabled {
                    Some(Ok(v.1.pool_type))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })
        .collect()
}

/// Returns general contract parameters using a custom [`ConfigResponse`] structure.
pub fn query_config(deps: Deps<CoreumQueries>) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        owner: config.owner,
        pool_configs: PAIR_CONFIGS
            .range(deps.storage, None, None, Order::Ascending)
            .map(|item| Ok(item?.1))
            .collect::<StdResult<Vec<_>>>()?,
        fee_address: config.fee_address,
        max_referral_commission: config.max_referral_commission,
        only_owner_can_create_pools: config.only_owner_can_create_pools,
        trading_starts: config.trading_starts,
    };

    Ok(resp)
}

/// Returns a pair's data using the assets in `asset_infos` as input (those being the assets that are traded in the pair).
/// * **asset_infos** is a vector with assets traded in the pair.
pub fn query_pair(deps: Deps<CoreumQueries>, asset_infos: Vec<AssetInfo>) -> StdResult<PairInfo> {
    let asset_infos = asset_infos
        .into_iter()
        .map(|a| a.validate(deps.api))
        .collect::<StdResult<Vec<_>>>()?;
    let pair_addr = PAIRS.load(deps.storage, &pair_key(&asset_infos))?;
    query_pair_info(&deps.querier, pair_addr)
}

/// Returns a vector with pair data that contains items of type [`PoolInfo`]. Querying starts at `start_after` and returns `limit` pairs.
/// * **start_after** is a field which accepts a vector with items of type [`AssetInfo`].
/// This is the pair from which we start a query.
///
/// * **limit** sets the number of pairs to be retrieved.
pub fn query_pairs(
    deps: Deps<CoreumQueries>,
    start_after: Option<Vec<AssetInfo>>,
    limit: Option<u32>,
) -> StdResult<PoolsResponse> {
    let pools = read_pairs(deps, start_after, limit)?
        .iter()
        .map(|pair_addr| query_pair_info(&deps.querier, pair_addr))
        .collect::<StdResult<Vec<_>>>()?;

    Ok(PoolsResponse { pools })
}

/// Returns the fee setup for a specific pair type using a [`FeeInfoResponse`] struct.
/// * **pool_type** is a struct that represents the fee information (total and protocol fees) for a specific pair type.
pub fn query_fee_info(
    deps: Deps<CoreumQueries>,
    pool_type: PoolType,
) -> StdResult<FeeInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let pair_config = PAIR_CONFIGS.load(deps.storage, pool_type.to_string())?;

    Ok(FeeInfoResponse {
        fee_address: config.fee_address,
        total_fee_bps: pair_config.fee_config.total_fee_bps,
        protocol_fee_bps: pair_config.fee_config.protocol_fee_bps,
    })
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    deps: DepsMut<CoreumQueries>,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response, ContractError> {
    match msg {
        MigrateMsg::Update() => {
            ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        }
        MigrateMsg::AddPermissionlessPoolDeposit(asset) => {
            PERMISSIONLESS_DEPOSIT_REQUIREMENT.save(deps.storage, &asset)?;
        }
    };

    Ok(Response::new())
}

/// Queryes a pool by it's address
///
/// Returns `true` if the pool is verified
/// Returns `false` if the pool is non-verified
pub fn query_pool_type(deps: Deps<CoreumQueries>, address: Addr) -> StdResult<bool> {
    deps.api.addr_validate(address.as_str())?;
    POOL_TYPES.load(deps.storage, address)
}
