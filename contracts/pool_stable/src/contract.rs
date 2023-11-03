use std::str::FromStr;
use std::vec;

use coreum_wasm_sdk::{
    assetft,
    core::{CoreumMsg, CoreumQueries},
};
use cosmwasm_std::{
    attr, coin, ensure, entry_point, from_binary, to_binary, Addr, BankMsg, Binary, Coin,
    CosmosMsg, Decimal, Decimal256, Deps, DepsMut, Env, Isqrt, MessageInfo, Reply, StdError,
    StdResult, Uint128, Uint256,
};

use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;

use dex::{
    asset::{
        addr_opt_validate, check_swap_parameters, format_lp_token_name, Asset, AssetInfoValidated,
        AssetValidated, MINIMUM_LIQUIDITY_AMOUNT,
    },
    decimal2decimal256,
    factory::PoolType,
    fee_config::FeeConfig,
    pool::{
        assert_max_spread, check_asset_infos, check_assets, check_cw20_in_pool,
        get_share_in_assets, handle_reply, save_tmp_staking_config, ConfigResponse, ContractError,
        CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PairInfo,
        PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse, StablePoolParams,
        DEFAULT_SLIPPAGE, LP_TOKEN_PRECISION, MAX_ALLOWED_SLIPPAGE, TWAP_PRECISION,
    },
};

use crate::{
    math::{calc_y, compute_d, AMP_PRECISION, MAX_AMP, MAX_AMP_CHANGE, MIN_AMP_CHANGING_TIME},
    state::{
        get_precision, store_precisions, Config, CIRCUIT_BREAKER, CONFIG, FROZEN, LP_SHARE_AMOUNT,
    },
};

pub type Response = cosmwasm_std::Response<CoreumMsg>;
pub type SubMsg = cosmwasm_std::SubMsg<CoreumMsg>;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "dex-stable-pool";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let asset_infos = check_asset_infos(deps.api, &msg.asset_infos)?;

    if asset_infos.len() != 2 {
        return Err(ContractError::InvalidNumberOfAssets { min: 2, max: 2 });
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    msg.validate_fees()?;

    // let factory_addr = deps.api.addr_validate(msg.factory_addr.as_str())?;

    let lp_token_name = format_lp_token_name(&asset_infos, &deps.querier)?;

    if msg.init_params.is_none() {
        return Err(ContractError::InitParamsNotFound {});
    }

    msg.validate_fees()?;

    let params: StablePoolParams = from_binary(&msg.init_params.unwrap())?;

    if params.amp == 0 || params.amp > MAX_AMP {
        return Err(ContractError::IncorrectAmp { max_amp: MAX_AMP });
    }
    let greatest_precision = store_precisions(deps.branch(), &asset_infos)?;

    // Initializing cumulative prices
    let mut cumulative_prices = vec![];
    for from_pool in &asset_infos {
        for to_pool in &asset_infos {
            if !from_pool.eq(to_pool) {
                cumulative_prices.push((from_pool.clone(), to_pool.clone(), Uint128::zero()))
            }
        }
    }

    let config = Config {
        owner: addr_opt_validate(deps.api, &params.owner)?,
        pool_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: format!("u{}-{}", lp_token_name.clone(), env.contract.address),
            staking_addr: Addr::unchecked(""),
            asset_infos,
            pool_type: PoolType::Stable {},
            fee_config: msg.fee_config,
        },
        // factory_addr,
        block_time_last: 0,
        init_amp: params.amp * AMP_PRECISION,
        init_amp_time: env.block.time.seconds(),
        next_amp: params.amp * AMP_PRECISION,
        next_amp_time: env.block.time.seconds(),
        greatest_precision,
        cumulative_prices,
        trading_starts: msg.trading_starts,
    };

    CONFIG.save(deps.storage, &config)?;
    FROZEN.save(deps.storage, &false)?;
    LP_SHARE_AMOUNT.save(deps.storage, &Uint128::zero())?;
    save_tmp_staking_config(deps.storage, &msg.staking_config)?;

    Ok(
        Response::new().add_submessage(SubMsg::new(CoreumMsg::AssetFT(assetft::Msg::Issue {
            symbol: lp_token_name.clone(),
            subunit: format!("u{}", lp_token_name),
            precision: LP_TOKEN_PRECISION,
            initial_amount: Uint128::zero(),
            description: Some("Dex LP Share token".to_string()),
            features: Some(vec![0, 1, 2]), // 0 - minting, 1 - burning, 2 - freezing
            burn_rate: Some("0".into()),
            send_commission_rate: Some("0.00000".into()),
        }))),
    )
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    deps: DepsMut<CoreumQueries>,
    _env: Env,
    msg: MigrateMsg,
) -> Result<Response, ContractError> {
    match msg {
        MigrateMsg::UpdateFreeze {
            frozen,
            circuit_breaker,
        } => {
            FROZEN.save(deps.storage, &frozen)?;
            if let Some(circuit_breaker) = circuit_breaker {
                CIRCUIT_BREAKER.save(deps.storage, &deps.api.addr_validate(&circuit_breaker)?)?;
            }
        }
    }

    Ok(Response::new())
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(
    deps: DepsMut<CoreumQueries>,
    _env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let res = handle_reply(&deps, msg, &mut config.pool_info)?;
    CONFIG.save(deps.storage, &config)?;

    Ok(res)
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Not supported.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::ProvideLiquidity {
///             assets,
///             slippage_tolerance,
///             receiver,
///         }** Provides liquidity in the pool with the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs a swap operation with the specified parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
            receiver,
        } => provide_liquidity(deps, env, info, assets, slippage_tolerance, receiver),
        ExecuteMsg::UpdateFees { fee_config } => update_fees(deps, info, fee_config),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
            referral_address,
            referral_commission,
            ..
        } => {
            let offer_asset = offer_asset.validate(deps.api)?;
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = addr_opt_validate(deps.api, &to)?;
            let referral_address = addr_opt_validate(deps.api, &referral_address)?;

            swap(
                deps,
                env,
                info.clone(),
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
                referral_address,
                referral_commission,
            )
        }
        ExecuteMsg::Freeze { frozen } => {
            ensure!(
                info.sender
                    == CIRCUIT_BREAKER
                        .may_load(deps.storage)?
                        .unwrap_or_else(|| Addr::unchecked("")),
                ContractError::Unauthorized {}
            );
            FROZEN.save(deps.storage, &frozen)?;
            Ok(Response::new())
        }
        ExecuteMsg::WithdrawLiquidity {} => withdraw_liquidity(deps, env, info),
        _ => Err(ContractError::NonSupported {}),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 receive message to process.
pub fn receive_cw20(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
            referral_address,
            referral_commission,
            ..
        } => {
            // Only asset contract can execute this message
            check_cw20_in_pool(
                &CONFIG.load(deps.storage)?.pool_info.asset_infos,
                &info.sender,
            )?;

            let to_addr = addr_opt_validate(deps.api, &to)?;
            let referral_address = addr_opt_validate(deps.api, &referral_address)?;
            let contract_addr = info.sender.clone();
            let sender = deps.api.addr_validate(&cw20_msg.sender)?;
            swap(
                deps,
                env,
                info,
                sender,
                AssetValidated {
                    info: AssetInfoValidated::Cw20Token(contract_addr),
                    amount: cw20_msg.amount,
                },
                belief_price,
                max_spread,
                to_addr,
                referral_address,
                referral_commission,
            )
        }
    }
}

pub fn update_fees(
    deps: DepsMut<CoreumQueries>,
    _info: MessageInfo,
    fee_config: FeeConfig,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    check_if_frozen(&deps)?;

    // check permissions
    // if info.sender != config.factory_addr {
    //     return Err(ContractError::Unauthorized {});
    // }

    // update config
    config.pool_info.fee_config = fee_config;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Provides liquidity in the pool with the specified input parameters.
///
/// * **assets** is an array with assets available in the pool.
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
///
/// * **receiver** is an optional parameter which defines the receiver of the LP tokens.
/// If no custom receiver is specified, the pool will mint LP tokens for the function caller.
///
/// NOTE - the address that wants to provide liquidity should approve the pool contract to pull its relevant tokens.
pub fn provide_liquidity(
    mut deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    check_if_frozen(&deps)?;
    let assets = check_assets(deps.api, &assets)?;
    let mut config = CONFIG.load(deps.storage)?;

    if assets.len() > config.pool_info.asset_infos.len() {
        return Err(ContractError::TooManyAssets {
            max: config.pool_info.asset_infos.len(),
            provided: assets.len(),
        });
    }

    let save_config = update_target_rate(deps.querier, &mut config, &env)?;

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut non_zero_flag = false;

    let mut assets_collection = assets
        .clone()
        .into_iter()
        .map(|asset| {
            asset.assert_sent_native_token_balance(&info)?;

            // Check that at least one asset is non-zero
            if !asset.amount.is_zero() {
                non_zero_flag = true;
            }

            // Get appropriate pool
            let pool = pools
                .get(&asset.info)
                .copied()
                .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

            Ok((asset, pool))
        })
        .collect::<Result<Vec<_>, ContractError>>()?;

    // If some assets are omitted then add them explicitly with 0 deposit
    pools.iter().for_each(|(pool_info, pool_amount)| {
        if !assets.iter().any(|asset| asset.info.eq(pool_info)) {
            assets_collection.push((
                AssetValidated {
                    amount: Uint128::zero(),
                    info: pool_info.clone(),
                },
                *pool_amount,
            ));
        }
    });

    if !non_zero_flag {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut messages = vec![];
    for (deposit, pool) in assets_collection.iter_mut() {
        // We cannot put a zero amount into an empty pool.
        if deposit.amount.is_zero() && pool.is_zero() {
            return Err(ContractError::InvalidProvideLPsWithSingleToken {});
        }

        // Transfer only non-zero amount
        if !deposit.amount.is_zero() {
            // If the pool is a token contract, then we need to execute a TransferFrom msg to receive funds
            if let AssetInfoValidated::Token(contract_addr) = &deposit.info {
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: deposit.amount,
                    })?,
                    funds: vec![],
                }))
            } else {
                // If the asset is a native token, the pool balance already increased
                // To calculate the pool balance properly, we should subtract the user deposit from the recorded pool token amount
                *pool = pool.checked_sub(deposit.amount)?;
            }
        }
    }

    let assets_collection = assets_collection
        .iter()
        .cloned()
        .map(|(asset, pool)| {
            let coin_precision = get_precision(deps.storage, &asset.info)?;
            Ok((
                asset.to_decimal_asset(coin_precision)?,
                Decimal256::with_precision(pool, coin_precision)?,
            ))
        })
        .collect::<StdResult<Vec<(DecimalAsset, Decimal256)>>>()?;

    let n_coins = config.pair_info.asset_infos.len() as u8;

    let amp = compute_current_amp(&config, &env)?;

    // Initial invariant (D)
    let old_balances = assets_collection
        .iter()
        .map(|(_, pool)| *pool)
        .collect_vec();
    let init_d = compute_d(amp, &old_balances, config.greatest_precision)?;

    // Invariant (D) after deposit added
    let mut new_balances: Vec<_> = assets_collection
        .iter()
        .map(|(deposit, pool)| Ok(pool + deposit.amount))
        .collect::<StdResult<Vec<_>>>()?;
    let deposit_d = compute_d(amp, &new_balances, config.greatest_precision)?;

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let share = if total_share.is_zero() {
        let share = deposit_d
            .to_uint128_with_precision(config.greatest_precision)?
            .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
            .map_err(|_| ContractError::MinimumLiquidityAmountError {})?;

        messages.extend(mint_token_message(
            &config.pair_info.liquidity_token,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
        )?);

        // share cannot become zero after minimum liquidity subtraction
        if share.is_zero() {
            return Err(ContractError::MinimumLiquidityAmountError {});
        }

        share
    } else {
        // Get fee info from the factory
        let fee_info = query_fee_info(
            &deps.querier,
            &config.factory_addr,
            config.pair_info.pair_type.clone(),
        )?;

        // total_fee_rate * N_COINS / (4 * (N_COINS - 1))
        let fee = fee_info
            .total_fee_rate
            .checked_mul(Decimal::from_ratio(n_coins, 4 * (n_coins - 1)))?;

        let fee = Decimal256::new(fee.atomics().into());

        for i in 0..n_coins as usize {
            let ideal_balance = deposit_d.checked_multiply_ratio(old_balances[i], init_d)?;
            let difference = if ideal_balance > new_balances[i] {
                ideal_balance - new_balances[i]
            } else {
                new_balances[i] - ideal_balance
            };
            // Fee will be charged only during imbalanced provide i.e. if invariant D was changed
            new_balances[i] -= fee.checked_mul(difference)?;
        }

        let after_fee_d = compute_d(amp, &new_balances, config.greatest_precision)?;

        let share = Decimal256::with_precision(total_share, config.greatest_precision)?
            .checked_multiply_ratio(after_fee_d.saturating_sub(init_d), init_d)?
            .to_uint128_with_precision(config.greatest_precision)?;

        if share.is_zero() {
            return Err(ContractError::LiquidityAmountTooSmall {});
        }

        share
    };

    // Mint LP token for the caller (or for the receiver if it was set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    messages.extend(mint_token_message(
        &config.pair_info.liquidity_token,
        &receiver,
        share,
    )?);

    // using assets_collection, since the deposit amount is already subtracted there
    let old_pools = assets_collection
        .iter()
        .map(|(a, p)| DecimalAsset {
            info: a.info.clone(),
            amount: *p,
        })
        .collect::<Vec<_>>();

    // calculate pools with deposited balances
    let new_pools = assets_collection
        .into_iter()
        .map(|(mut asset, pool)| {
            // add deposit amount back to pool amount, so we can calculate the new price
            asset.amount += pool;
            asset
        })
        .collect::<Vec<_>>();
    let new_price = calc_new_price_a_per_b(deps.as_ref(), &env, &config, &new_pools)?;

    if total_share.is_zero() {
        // initialize oracle storage
        dex::oracle::initialize_oracle(deps.storage, &env, new_price)?;
    } else {
        dex::oracle::store_oracle_price(deps.storage, &env, new_price)?;
    }

    if accumulate_prices(deps.as_ref(), &env, &mut config, &old_pools)? || save_config {
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", assets.iter().join(", ")),
        attr("share", share),
    ]))
}

/// Withdraw liquidity from the pool.
/// * **sender** is the address that will receive assets back from the pool contract.
///
/// * **amount** is the amount of LP tokens to burn.
pub fn withdraw_liquidity(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage).unwrap();

    if info.funds[0].denom.clone() != config.pool_info.liquidity_token.clone() {
        return Err(ContractError::Unauthorized {});
    }

    let sender = info.sender.clone();
    let amount = info.funds[0].amount;

    let (pools, total_share) = pool_info(deps.as_ref(), &config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    // Calculate new pool amounts
    let mut new_pools = pools
        .iter()
        .zip(refund_assets.iter())
        .map(|(p, r)| p.amount - r.amount);
    let (new_pool0, new_pool1) = (new_pools.next().unwrap(), new_pools.next().unwrap());
    dex::oracle::store_oracle_price(
        deps.storage,
        &env,
        Decimal::from_ratio(new_pool0, new_pool1),
    )?;

    // Accumulate prices for the pool assets
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) =
        accumulate_prices(&env, &config, pools[0].amount, pools[1].amount)?
    {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    // Update the pool info
    let messages: Vec<CosmosMsg<CoreumMsg>> = vec![
        refund_assets[0].clone().into_msg(sender.clone())?,
        refund_assets[1].clone().into_msg(sender.clone())?,
        CosmosMsg::Custom(CoreumMsg::AssetFT(assetft::Msg::Burn {
            coin: coin(amount.u128(), &config.pool_info.liquidity_token),
        })),
    ];
    LP_SHARE_AMOUNT.update(deps.storage, |mut amount| -> StdResult<_> {
        amount -= amount;
        Ok(amount)
    })?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", sender),
        attr("withdrawn_share", amount),
        attr(
            "refund_assets",
            format!("{}, {}", refund_assets[0], refund_assets[1]),
        ),
    ]))
}

/// Performs an swap operation with the specified parameters. The trader must approve the
/// pool contract to transfer offer assets from their wallet.
///
/// * **sender** is the sender of the swap operation.
///
/// * **offer_asset** proposed asset for swapping.
///
/// * **belief_price** is used to calculate the maximum swap spread.
///
/// * **max_spread** sets the maximum spread of the swap operation.
///
/// * **to** sets the recipient of the swap operation.
///
/// NOTE - the address that wants to swap should approve the pool contract to pull the offer token.
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut<CoreumQueries>,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    offer_asset: AssetValidated,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
    _referral_address: Option<Addr>,
    _referral_commission: Option<Decimal>,
) -> Result<Response, ContractError> {
    offer_asset.assert_sent_native_token_balance(&info)?;
    let original_offer_asset = offer_asset.clone();

    check_if_frozen(&deps)?;

    let mut config = CONFIG.load(deps.storage)?;
    // Get config from the factory
    // let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    let mut messages: Vec<CosmosMsg<CoreumMsg>> = Vec::new();

    // handle_referral(
    //     &factory_config,
    //     referral_address,
    //     referral_commission,
    //     &mut offer_asset,
    //     &mut messages,
    // )?;

    // If the asset balance is already increased, we should subtract the user deposit from the pool amount
    let pools = config
        .pool_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|mut p| {
            if p.info.equal(&original_offer_asset.info) {
                p.amount = p.amount.checked_sub(original_offer_asset.amount)?;
            }
            Ok(p)
        })
        .collect::<StdResult<Vec<_>>>()?;

    let SwapResult {
        return_asset,
        ask_info,
        spread_amount,
        commission_amount,
        protocol_fee_amount,
        protocol_fee_msg,
    } = do_swap(
        deps,
        &env,
        &mut config,
        // &factory_config,
        &pools,
        &offer_asset,
        belief_price,
        max_spread,
    )?;

    let receiver = to.unwrap_or_else(|| sender.clone());
    messages.push(return_asset.into_msg(&receiver)?);

    if let Some(msg) = protocol_fee_msg {
        messages.push(msg);
    }

    Ok(Response::new()
        .add_messages(
            // 1. send collateral tokens from the contract to a user
            // 2. send inactive commission fees to the protocol
            messages,
        )
        .add_attributes(vec![
            attr("action", "swap"),
            attr("sender", sender),
            attr("receiver", receiver),
            attr("offer_asset", offer_asset.info.to_string()),
            attr("ask_asset", ask_info.to_string()),
            attr("offer_amount", offer_asset.amount),
            attr("return_amount", return_asset.amount),
            attr("spread_amount", spread_amount),
            attr("commission_amount", commission_amount),
            attr("protocol_fee_amount", protocol_fee_amount),
        ]))
}

fn check_if_frozen(deps: &DepsMut<CoreumQueries>) -> Result<(), ContractError> {
    let is_frozen: bool = FROZEN.load(deps.storage)?;
    ensure!(!is_frozen, ContractError::ContractFrozen {});
    Ok(())
}

struct SwapResult {
    return_asset: AssetValidated,
    ask_info: AssetInfoValidated,
    spread_amount: Uint128,
    commission_amount: Uint128,
    protocol_fee_amount: Uint128,
    protocol_fee_msg: Option<CosmosMsg<CoreumMsg>>,
}
/// Helper method that executes a swap of one asset for another without needing to receive or send out the coins.
/// Instead it returns the amount of the ask asset, as well as the protocol fee.
/// This method is useful for swapping in the middle of another message, where the coins are already in the contract.
///
/// Important: When providing the pool balances for this method, make sure that those do *not* include the offer asset.
#[allow(clippy::too_many_arguments)]
fn do_swap(
    deps: DepsMut<CoreumQueries>,
    env: &Env,
    config: &mut Config,
    // factory_config: &FactoryConfig,
    pools: &[AssetValidated],
    offer_asset: &AssetValidated,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
) -> Result<SwapResult, ContractError> {
    if env.block.time.seconds() < config.trading_starts {
        return Err(ContractError::TradingNotStarted {});
    }

    let offer_pool: AssetValidated;
    let ask_pool: AssetValidated;

    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
    }

    let offer_amount = offer_asset.amount;

    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_amount,
        config.pool_info.fee_config.total_fee_rate(),
    )?;

    // Check the max spread limit (if it was specified)
    assert_max_spread(
        belief_price,
        max_spread,
        offer_amount,
        return_amount + commission_amount,
        spread_amount,
    )?;

    // Compute the tax for the receiving asset (if it is a native one)
    let return_asset = AssetValidated {
        info: ask_pool.info.clone(),
        amount: return_amount,
    };

    // Compute the protocol fee
    let fee_msg = None;
    let protocol_fee_amount = Uint128::zero();
    // if let Some(ref fee_address) = factory_config.fee_address {
    //     if let Some(f) = calculate_protocol_fee(
    //         &ask_pool.info,
    //         commission_amount,
    //         config.pool_info.fee_config.protocol_fee_rate(),
    //     ) {
    //         protocol_fee_amount = f.amount;
    //         fee_msg = Some(f.into_msg(fee_address)?);
    //     }
    // }

    // Calculate new pool amounts
    let (new_pool0, new_pool1) = if pools[0].info.equal(&ask_pool.info) {
        // subtract fee and return amount from ask pool
        // add offer amount to offer pool
        (
            pools[0].amount - protocol_fee_amount - return_amount,
            pools[1].amount + offer_amount,
        )
    } else {
        // same as above, but with inverted indices
        (
            pools[0].amount + offer_amount,
            pools[1].amount - protocol_fee_amount - return_amount,
        )
    };
    dex::oracle::store_oracle_price(deps.storage, env, Decimal::from_ratio(new_pool0, new_pool1))?;

    // Accumulate prices for the assets in the pool
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) =
        accumulate_prices(env, config, pools[0].amount, pools[1].amount)?
    {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, config)?;
    }

    Ok(SwapResult {
        return_asset,
        ask_info: ask_pool.info,
        spread_amount,
        commission_amount,
        protocol_fee_amount,
        protocol_fee_msg: fee_msg,
    })
}

/// Accumulate token prices for the assets in the pool.
/// Note that this function shifts **block_time** when any of the token prices is zero in order to not
/// fill an accumulator with a null price for that period.
///
/// * **x** is the balance of asset\[\0] in the pool.
///
/// * **y** is the balance of asset\[\1] in the pool.
pub fn accumulate_prices(
    env: &Env,
    config: &Config,
    x: Uint128,
    y: Uint128,
) -> StdResult<Option<(Uint128, Uint128, u64)>> {
    let block_time = env.block.time.seconds();
    if block_time <= config.block_time_last {
        return Ok(None);
    }

    // We have to shift block_time when any price is zero in order to not fill an accumulator with a null price for that period
    let time_elapsed = Uint128::from(block_time - config.block_time_last);

    let mut pcl0 = config.price0_cumulative_last;
    let mut pcl1 = config.price1_cumulative_last;

    if !x.is_zero() && !y.is_zero() {
        let price_precision = Uint128::from(10u128.pow(TWAP_PRECISION.into()));
        pcl0 = config.price0_cumulative_last.wrapping_add(
            time_elapsed
                .checked_mul(price_precision)?
                .multiply_ratio(y, x),
        );
        pcl1 = config.price1_cumulative_last.wrapping_add(
            time_elapsed
                .checked_mul(price_precision)?
                .multiply_ratio(x, y),
        );
    };

    Ok(Some((pcl0, pcl1, block_time)))
}

/// Calculates the amount of fees the protocol gets according to specified pool parameters.
/// Returns a [`None`] if the protocol fee is zero, otherwise returns a [`Asset`] struct with the specified attributes.
///
/// * **pool_info** contains information about the pool asset for which the commission will be calculated.
///
/// * **commission_amount** is the total amount of fees charged for a swap.
///
/// * **protocol_commission_rate** is the percentage of fees that go to the protocol.
pub fn calculate_protocol_fee(
    pool_info: &AssetInfoValidated,
    commission_amount: Uint128,
    protocol_commission_rate: Decimal,
) -> Option<AssetValidated> {
    let protocol_fee: Uint128 = commission_amount * protocol_commission_rate;
    if protocol_fee.is_zero() {
        return None;
    }

    Some(AssetValidated {
        info: pool_info.clone(),
        amount: protocol_fee,
    })
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Pool {}** Returns information about the pool in an object of type [`PairInfo`].
///
/// * **QueryMsg::Pool {}** Returns information about the amount of assets in the pool contract as
/// well as the amount of LP tokens issued using an object of type [`PoolResponse`].
///
/// * **QueryMsg::Share { amount }** Returns the amount of assets that could be withdrawn from the pool
/// using a specific amount of LP tokens. The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **QueryMsg::Simulation { offer_asset }** Returns the result of a swap simulation using a [`SimulationResponse`] object.
///
/// * **QueryMsg::ReverseSimulation { ask_asset }** Returns the result of a reverse swap simulation  using
/// a [`ReverseSimulationResponse`] object.
///
/// * **QueryMsg::CumulativePrices {}** Returns information about cumulative prices for the assets in the
/// pool using a [`CumulativePricesResponse`] object.
///
/// * **QueryMsg::HistoricalPrices { duration }** Returns historical price information for the assets in the
/// pool using a [`HistoricalPricesResponse`] object.
///
/// * **QueryMsg::Config {}** Returns the configuration for the pool contract using a [`ConfigResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<CoreumQueries>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&CONFIG.load(deps.storage)?.pool_info),
        QueryMsg::Pool {} => to_binary(&query_pool(deps)?),
        QueryMsg::Share { amount } => to_binary(&query_share(deps, amount)?),
        QueryMsg::Simulation {
            offer_asset,
            referral,
            referral_commission,
            ..
        } => to_binary(&query_simulation(
            deps,
            offer_asset,
            referral,
            referral_commission,
        )?),
        QueryMsg::ReverseSimulation {
            ask_asset,
            referral,
            referral_commission,
            ..
        } => to_binary(&query_reverse_simulation(
            deps,
            ask_asset,
            referral,
            referral_commission,
        )?),
        QueryMsg::CumulativePrices {} => to_binary(&query_cumulative_prices(deps, env)?),
        QueryMsg::Twap {
            duration,
            start_age,
            end_age,
        } => to_binary(&dex::oracle::query_oracle_range(
            deps.storage,
            &env,
            &CONFIG.load(deps.storage)?.pool_info.asset_infos,
            duration,
            start_age,
            end_age,
        )?),
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        _ => Err(StdError::generic_err("Query is not supported")),
    }
}

/// Returns the amounts of assets in the pool contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
pub fn query_pool(deps: Deps<CoreumQueries>) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps, &config)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

/// Returns the amount of assets that could be withdrawn from the pool using a specific amount of LP tokens.
/// The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **amount** is the amount of LP tokens for which we calculate associated amounts of assets.
pub fn query_share(deps: Deps<CoreumQueries>, amount: Uint128) -> StdResult<Vec<AssetValidated>> {
    let config = CONFIG.load(deps.storage)?;
    let (pools, total_share) = pool_info(deps, &config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    Ok(refund_assets)
}

/// Returns information about a swap simulation in a [`SimulationResponse`] object.
///
/// * **offer_asset** is the asset to swap as well as an amount of the said asset.
pub fn query_simulation(
    deps: Deps<CoreumQueries>,
    offer_asset: Asset,
    _referral: bool,
    _referral_commission: Option<Decimal>,
) -> StdResult<SimulationResponse> {
    let offer_asset = offer_asset.validate(deps.api)?;
    let config = CONFIG.load(deps.storage)?;

    let referral_amount = Uint128::zero() /*if referral {
        let factory_config = query_factory_config(&deps.querier, config.factory_addr)?;
        take_referral(&factory_config, referral_commission, &mut offer_asset)?
    } else {
        Uint128::zero()
    }*/;

    let pools = config
        .pool_info
        .query_pools(&deps.querier, &config.pool_info.contract_addr)?;

    let offer_pool: AssetValidated;
    let ask_pool: AssetValidated;
    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err(
            "Given offer asset does not belong in the pool",
        ));
    }

    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_asset.amount,
        config.pool_info.fee_config.total_fee_rate(),
    )?;

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
        referral_amount,
    })
}

/// Returns information about a reverse swap simulation in a [`ReverseSimulationResponse`] object.
///
/// * **ask_asset** is the asset to swap to as well as the desired amount of ask
/// assets to receive from the swap.
pub fn query_reverse_simulation(
    deps: Deps<CoreumQueries>,
    ask_asset: Asset,
    _referral: bool,
    _referral_commission: Option<Decimal>,
) -> StdResult<ReverseSimulationResponse> {
    let ask_asset = ask_asset.validate(deps.api)?;
    let config = CONFIG.load(deps.storage)?;

    let pools = config
        .pool_info
        .query_pools(&deps.querier, &config.pool_info.contract_addr)?;

    let offer_pool: AssetValidated;
    let ask_pool: AssetValidated;
    if ask_asset.info.equal(&pools[0].info) {
        ask_pool = pools[0].clone();
        offer_pool = pools[1].clone();
    } else if ask_asset.info.equal(&pools[1].info) {
        ask_pool = pools[1].clone();
        offer_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err(
            "Given ask asset doesn't belong to pools",
        ));
    }

    let (offer_amount, spread_amount, commission_amount) = compute_offer_amount(
        offer_pool.amount,
        ask_pool.amount,
        ask_asset.amount,
        config.pool_info.fee_config.total_fee_rate(),
    )?;

    // `offer_pool.info` is already validated
    let offer_asset = AssetValidated {
        info: offer_pool.info,
        amount: offer_amount,
    };
    // let (offer_asset, referral_amount) = add_referral(
    //     &deps.querier,
    //     &config.factory_addr,
    //     referral,
    //     referral_commission,
    //     offer_asset,
    // )?;

    Ok(ReverseSimulationResponse {
        offer_amount: offer_asset.amount,
        spread_amount,
        commission_amount,
        referral_amount: Uint128::zero(),
    })
}

/// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
pub fn query_cumulative_prices(
    deps: Deps<CoreumQueries>,
    env: Env,
) -> StdResult<CumulativePricesResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps, &config)?;

    let mut price0_cumulative_last = config.price0_cumulative_last;
    let mut price1_cumulative_last = config.price1_cumulative_last;

    if let Some((price0_cumulative_new, price1_cumulative_new, _)) =
        accumulate_prices(&env, &config, assets[0].amount, assets[1].amount)?
    {
        price0_cumulative_last = price0_cumulative_new;
        price1_cumulative_last = price1_cumulative_new;
    }

    let cumulative_prices = vec![
        (
            assets[0].info.clone(),
            assets[1].info.clone(),
            price0_cumulative_last,
        ),
        (
            assets[1].info.clone(),
            assets[0].info.clone(),
            price1_cumulative_last,
        ),
    ];

    let resp = CumulativePricesResponse {
        assets,
        total_share,
        cumulative_prices,
    };

    Ok(resp)
}

/// Returns the pool contract configuration in a [`ConfigResponse`] object.
pub fn query_config(deps: Deps<CoreumQueries>) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: None,
        owner: None,
    })
}

/// Returns the result of a swap.
///
/// * **offer_pool** total amount of offer assets in the pool.
///
/// * **ask_pool** total amount of ask assets in the pool.
///
/// * **offer_amount** amount of offer assets to swap.
///
/// * **commission_rate** total amount of fees charged for the swap.
pub fn compute_swap(
    offer_pool: Uint128,
    ask_pool: Uint128,
    offer_amount: Uint128,
    commission_rate: Decimal,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // offer => ask
    check_swap_parameters(vec![offer_pool, ask_pool], offer_amount)?;

    let offer_pool: Uint256 = offer_pool.into();
    let ask_pool: Uint256 = ask_pool.into();
    let offer_amount: Uint256 = offer_amount.into();
    let commission_rate = decimal2decimal256(commission_rate)?;

    // ask_amount = (ask_pool - cp / (offer_pool + offer_amount))
    let cp: Uint256 = offer_pool * ask_pool;
    let return_amount: Uint256 = (Decimal256::from_ratio(ask_pool, 1u8)
        - Decimal256::from_ratio(cp, offer_pool + offer_amount))
        * Uint256::from(1u8);

    // Calculate spread & commission
    let spread_amount: Uint256 =
        (offer_amount * Decimal256::from_ratio(ask_pool, offer_pool)) - return_amount;
    let commission_amount: Uint256 = return_amount * commission_rate;

    // The commision (minus the part that goes to the protocol) will be absorbed by the pool
    let return_amount: Uint256 = return_amount - commission_amount;
    Ok((
        return_amount.try_into()?,
        spread_amount.try_into()?,
        commission_amount.try_into()?,
    ))
}

/// Returns an amount of offer assets for a specified amount of ask assets.
///
/// * **offer_pool** total amount of offer assets in the pool.
///
/// * **ask_pool** total amount of ask assets in the pool.
///
/// * **ask_amount** amount of ask assets to swap to.
///
/// * **commission_rate** total amount of fees charged for the swap.
pub fn compute_offer_amount(
    offer_pool: Uint128,
    ask_pool: Uint128,
    ask_amount: Uint128,
    commission_rate: Decimal,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // ask => offer
    check_swap_parameters(vec![offer_pool, ask_pool], ask_amount)?;

    // offer_amount = cp / (ask_pool - ask_amount / (1 - commission_rate)) - offer_pool
    let cp = Uint256::from(offer_pool) * Uint256::from(ask_pool);
    let one_minus_commission = Decimal256::one() - decimal2decimal256(commission_rate)?;
    let inv_one_minus_commission = Decimal256::one() / one_minus_commission;

    let offer_amount: Uint128 = cp
        .multiply_ratio(
            Uint256::from(1u8),
            Uint256::from(
                ask_pool.checked_sub(
                    (Uint256::from(ask_amount) * inv_one_minus_commission).try_into()?,
                )?,
            ),
        )
        .checked_sub(offer_pool.into())?
        .try_into()?;

    let before_commission_deduction = Uint256::from(ask_amount) * inv_one_minus_commission;
    let spread_amount = (offer_amount * Decimal::from_ratio(ask_pool, offer_pool))
        .saturating_sub(before_commission_deduction.try_into()?);
    let commission_amount = before_commission_deduction * decimal2decimal256(commission_rate)?;
    Ok((offer_amount, spread_amount, commission_amount.try_into()?))
}

/// This is an internal function that enforces slippage tolerance for swaps.
///
/// * **slippage_tolerance** slippage tolerance to enforce.
///
/// * **deposits** array with offer and ask amounts for a swap.
///
/// * **pools** array with total amount of assets in the pool.
fn assert_slippage_tolerance(
    slippage_tolerance: Option<Decimal>,
    deposits: &[Uint128; 2],
    pools: &[AssetValidated],
) -> Result<(), ContractError> {
    let default_slippage = Decimal::from_str(DEFAULT_SLIPPAGE)?;
    let max_allowed_slippage = Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?;

    let slippage_tolerance = slippage_tolerance.unwrap_or(default_slippage);
    if slippage_tolerance.gt(&max_allowed_slippage) {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    let slippage_tolerance: Decimal256 = decimal2decimal256(slippage_tolerance)?;
    let one_minus_slippage_tolerance = Decimal256::one() - slippage_tolerance;
    let deposits: [Uint256; 2] = [deposits[0].into(), deposits[1].into()];
    let pools: [Uint256; 2] = [pools[0].amount.into(), pools[1].amount.into()];

    // Ensure each price does not change more than what the slippage tolerance allows
    if Decimal256::from_ratio(deposits[0], deposits[1]) * one_minus_slippage_tolerance
        > Decimal256::from_ratio(pools[0], pools[1])
        || Decimal256::from_ratio(deposits[1], deposits[0]) * one_minus_slippage_tolerance
            > Decimal256::from_ratio(pools[1], pools[0])
    {
        return Err(ContractError::MaxSlippageAssertion {});
    }

    Ok(())
}

/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
pub fn pool_info(
    deps: Deps<CoreumQueries>,
    config: &Config,
) -> StdResult<(Vec<AssetValidated>, Uint128)> {
    let pools = config
        .pool_info
        .query_pools(&deps.querier, &config.pool_info.contract_addr)?;
    // FIXME: For some reason this query doesn't work; use a local storage workaround
    // let total_share = query_supply(&deps.querier, &config.pool_info.liquidity_token)?;
    let total_share = LP_SHARE_AMOUNT.load(deps.storage)?;

    Ok((pools, total_share))
}

/// Updates the pool configuration with the specified parameters in the `params` variable.
///
/// * **params** new parameter values.
pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    if info.sender
        != if let Some(ref owner) = config.owner {
            owner.to_owned()
        } else {
            factory_config.owner
        }
    {
        return Err(ContractError::Unauthorized {});
    }

    match from_binary::<StablePoolUpdateParams>(&params)? {
        StablePoolUpdateParams::StartChangingAmp {
            next_amp,
            next_amp_time,
        } => start_changing_amp(config, deps, env, next_amp, next_amp_time)?,
        StablePoolUpdateParams::StopChangingAmp {} => stop_changing_amp(config, deps, env)?,
    }

    Ok(Response::default())
}

/// Start changing the AMP value.
///
/// * **next_amp** new value for AMP.
///
/// * **next_amp_time** end time when the pool amplification will be equal to `next_amp`.
fn start_changing_amp(
    mut config: Config,
    deps: DepsMut,
    env: Env,
    next_amp: u64,
    next_amp_time: u64,
) -> Result<(), ContractError> {
    if next_amp == 0 || next_amp > MAX_AMP {
        return Err(ContractError::IncorrectAmp { max_amp: MAX_AMP });
    }

    let current_amp = compute_current_amp(&config, &env)?.u64();

    let next_amp_with_precision = next_amp * AMP_PRECISION;

    if next_amp_with_precision * MAX_AMP_CHANGE < current_amp
        || next_amp_with_precision > current_amp * MAX_AMP_CHANGE
    {
        return Err(ContractError::MaxAmpChangeAssertion {
            max_amp_change: MAX_AMP_CHANGE,
        });
    }

    let block_time = env.block.time.seconds();

    if block_time < config.init_amp_time + MIN_AMP_CHANGING_TIME
        || next_amp_time < block_time + MIN_AMP_CHANGING_TIME
    {
        return Err(ContractError::MinAmpChangingTimeAssertion {
            min_amp_changing_time: MIN_AMP_CHANGING_TIME,
        });
    }

    config.init_amp = current_amp;
    config.next_amp = next_amp_with_precision;
    config.init_amp_time = block_time;
    config.next_amp_time = next_amp_time;

    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

/// Stop changing the AMP value.
fn stop_changing_amp(mut config: Config, deps: DepsMut, env: Env) -> StdResult<()> {
    let current_amp = compute_current_amp(&config, &env)?;
    let block_time = env.block.time.seconds();

    config.init_amp = current_amp.u64();
    config.next_amp = current_amp.u64();
    config.init_amp_time = block_time;
    config.next_amp_time = block_time;

    // now (block_time < next_amp_time) is always False, so we return the saved AMP
    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

/// Compute the current pool D value.
fn query_compute_d(deps: Deps, env: Env) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let amp = compute_current_amp(&config, &env)?;
    let pools = config
        .pair_info
        .query_pools_decimal(&deps.querier, env.contract.address)?
        .into_iter()
        .map(|pool| pool.amount)
        .collect::<Vec<_>>();

    compute_d(amp, &pools, config.greatest_precision)
        .map_err(|_| StdError::generic_err("Failed to calculate the D"))?
        .to_uint128_with_precision(config.greatest_precision)
}

/// Updates the config's target rate from the configured lsd hub contract if it is outdated.
/// Returns `true` if the target rate was updated, `false` otherwise.
fn update_target_rate(
    querier: QuerierWrapper<Empty>,
    config: &mut Config,
    env: &Env,
) -> StdResult<bool> {
    if let Some(lsd) = &mut config.lsd {
        let now = env.block.time.seconds();
        if now < lsd.last_target_query + lsd.target_rate_epoch {
            // target rate is up to date
            return Ok(false);
        }

        let response: TargetValueResponse =
            querier.query_wasm_smart(&lsd.lsd_hub, &TargetQuery::TargetValue {})?;

        lsd.target_rate = response.target_value;
        lsd.last_target_query = now;

        Ok(true)
    } else {
        Ok(false)
    }
}
