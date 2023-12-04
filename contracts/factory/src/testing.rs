use cosmwasm_std::{
    attr, from_json,
    testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR},
    to_json_binary, Addr, Decimal, ReplyOn, SubMsg, Uint128, WasmMsg,
};
use cw_utils::MsgInstantiateContractResponse;

use dex::{
    asset::AssetInfo,
    factory::{
        ConfigResponse, DefaultStakeConfig, ExecuteMsg, InstantiateMsg, PartialStakeConfig,
        PoolConfig, PoolType, PoolsResponse, QueryMsg,
    },
    fee_config::FeeConfig,
    pool::{InstantiateMsg as PoolInstantiateMsg, PairInfo},
};

use crate::{
    contract::{execute, instantiate, query, reply},
    error::ContractError,
    mock_querier::mock_dependencies,
    state::CONFIG,
};

fn default_stake_config() -> DefaultStakeConfig {
    DefaultStakeConfig {
        staking_code_id: 1234u64,
        tokens_per_power: Uint128::new(1000),
        min_bond: Uint128::new(1000),
        unbonding_periods: vec![1],
        max_distributions: 6,
    }
}

#[test]
fn pool_type_to_string() {
    assert_eq!(PoolType::Xyk {}.to_string(), "xyk");
    assert_eq!(PoolType::Stable {}.to_string(), "stable");
}

#[test]
fn proper_initialization() {
    // Validate total and protocol fee bps
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000".to_string();

    let msg = InstantiateMsg {
        pool_configs: vec![
            PoolConfig {
                code_id: 123u64,
                pool_type: PoolType::Xyk {},
                fee_config: FeeConfig {
                    total_fee_bps: 100,
                    protocol_fee_bps: 10,
                },
                is_disabled: false,
            },
            PoolConfig {
                code_id: 325u64,
                pool_type: PoolType::Xyk {},
                fee_config: FeeConfig {
                    total_fee_bps: 100,
                    protocol_fee_bps: 10,
                },
                is_disabled: false,
            },
        ],
        fee_address: None,
        owner: owner.clone(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::PoolConfigDuplicate {});

    let msg = InstantiateMsg {
        pool_configs: vec![PoolConfig {
            code_id: 123u64,
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 10_001,
                protocol_fee_bps: 10,
            },
            is_disabled: false,
        }],
        fee_address: None,
        owner: owner.clone(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::PoolConfigInvalidFeeBps {});

    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pool_configs: vec![
            PoolConfig {
                code_id: 325u64,
                pool_type: PoolType::Xyk {},
                fee_config: FeeConfig {
                    total_fee_bps: 100,
                    protocol_fee_bps: 10,
                },
                is_disabled: false,
            },
            PoolConfig {
                code_id: 123u64,
                pool_type: PoolType::Xyk {},
                fee_config: FeeConfig {
                    total_fee_bps: 100,
                    protocol_fee_bps: 10,
                },
                is_disabled: false,
            },
        ],
        fee_address: None,
        owner: owner.clone(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    instantiate(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();

    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_json(&query_res).unwrap();
    assert_eq!(msg.pool_configs, config_res.pool_configs);
    assert_eq!(Addr::unchecked(owner), config_res.owner);
}

#[test]
fn trading_starts_validation() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    let owner = "owner";

    let mut msg = InstantiateMsg {
        pool_configs: vec![],
        fee_address: None,
        owner: owner.to_string(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    // in the past
    msg.trading_starts = Some(env.block.time.seconds() - 1);
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
    assert_eq!(res, ContractError::InvalidTradingStart {});

    const SECONDS_PER_DAY: u64 = 60 * 60 * 24;
    // too late
    msg.trading_starts = Some(env.block.time.seconds() + 60 * SECONDS_PER_DAY + 1);
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
    assert_eq!(res, ContractError::InvalidTradingStart {});

    // just before too late
    msg.trading_starts = Some(env.block.time.seconds() + 60 * SECONDS_PER_DAY);
    instantiate(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();

    // right now
    msg.trading_starts = Some(env.block.time.seconds());
    instantiate(deps.as_mut(), env, info, msg).unwrap();
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let pool_configs = vec![PoolConfig {
        code_id: 123u64,
        pool_type: PoolType::Xyk {},
        fee_config: FeeConfig {
            total_fee_bps: 3,
            protocol_fee_bps: 166,
        },
        is_disabled: false,
    }];

    let msg = InstantiateMsg {
        pool_configs,
        fee_address: None,
        owner: owner.to_string(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info(owner, &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Update config
    let env = mock_env();
    let info = mock_info(owner, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        fee_address: Some(String::from("new_fee_addr")),
        only_owner_can_create_pools: Some(true),
        default_stake_config: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_json(&query_res).unwrap();
    assert_eq!(owner, config_res.owner);
    assert_eq!(
        String::from("new_fee_addr"),
        config_res.fee_address.unwrap()
    );

    // Unauthorized err
    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        fee_address: None,
        only_owner_can_create_pools: None,
        default_stake_config: None,
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});
}

#[test]
fn update_owner() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let msg = InstantiateMsg {
        pool_configs: vec![],
        fee_address: None,
        owner: owner.to_string(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info(owner, &[]);

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    let new_owner = String::from("new_owner");

    // New owner
    let env = mock_env();
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    let info = mock_info(new_owner.as_str(), &[]);

    // Unauthorized check
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let info = mock_info(new_owner.as_str(), &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();

    // Propose new owner
    let info = mock_info(owner, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Unauthorized ownership claim
    let info = mock_info("invalid_addr", &[]);
    let err = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim ownership
    let info = mock_info(new_owner.as_str(), &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    // Let's query the state
    let config: ConfigResponse =
        from_json(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(new_owner, config.owner);
}

#[test]
fn update_pair_config() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";
    let pool_configs = vec![PoolConfig {
        pool_type: PoolType::Xyk {},
        fee_config: FeeConfig {
            total_fee_bps: 100,
            protocol_fee_bps: 10,
        },
        is_disabled: false,
    }];

    let msg = InstantiateMsg {
        pool_configs: pool_configs.clone(),
        fee_address: None,
        owner: owner.to_string(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_json(&query_res).unwrap();
    assert_eq!(pool_configs, config_res.pool_configs);

    // Update config
    let pair_config = PoolConfig {
        pool_type: PoolType::Xyk {},
        fee_config: FeeConfig {
            total_fee_bps: 1,
            protocol_fee_bps: 2,
        },
        is_disabled: false,
    };

    // Unauthorized err
    let env = mock_env();
    let info = mock_info("wrong-addr0000", &[]);
    let msg = ExecuteMsg::UpdatePoolConfig {
        config: pair_config.clone(),
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // Check validation of total and protocol fee bps
    let env = mock_env();
    let info = mock_info(owner, &[]);
    let msg = ExecuteMsg::UpdatePoolConfig {
        config: PoolConfig {
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 3,
                protocol_fee_bps: 10_001,
            },
            is_disabled: false,
        },
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
    assert_eq!(res, ContractError::PoolConfigInvalidFeeBps {});

    let info = mock_info(owner, &[]);
    let msg = ExecuteMsg::UpdatePoolConfig {
        config: pair_config.clone(),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_json(&query_res).unwrap();
    assert_eq!(vec![pair_config.clone()], config_res.pool_configs);

    // Add second config
    let pair_config_custom = PoolConfig {
        pool_type: PoolType::Custom("test".to_string()),
        fee_config: FeeConfig {
            total_fee_bps: 10,
            protocol_fee_bps: 20,
        },
        is_disabled: false,
    };

    let info = mock_info(owner, &[]);
    let msg = ExecuteMsg::UpdatePoolConfig {
        config: pair_config_custom.clone(),
    };

    execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // It worked, let's query the state
    let query_res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config_res: ConfigResponse = from_json(&query_res).unwrap();
    assert_eq!(
        vec![pair_config_custom, pair_config],
        config_res.pool_configs
    );
}

#[test]
fn create_pair() {
    let mut deps = mock_dependencies(&[]);

    let pair_config = PoolConfig {
        pool_type: PoolType::Xyk {},
        fee_config: FeeConfig {
            total_fee_bps: 100,
            protocol_fee_bps: 10,
        },
        is_disabled: false,
    };

    let msg = InstantiateMsg {
        pool_configs: vec![pair_config.clone()],
        fee_address: None,
        owner: "owner0000".to_string(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);

    // We can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), env, info, msg.clone()).unwrap();

    let asset_infos = vec![
        AssetInfo::Cw20Token("asset0000".to_string()),
        AssetInfo::Cw20Token("asset0001".to_string()),
    ];

    let config = CONFIG.load(&deps.storage);
    let env = mock_env();
    let info = mock_info("owner0000", &[]);

    // Check pair creation using a non-whitelisted pair ID
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::CreatePool {
            pool_type: PoolType::Xyk {},
            asset_infos: asset_infos.clone(),
            init_params: None,
            total_fee_bps: None,
            staking_config: PartialStakeConfig::default(),
        },
    )
    .unwrap_err();
    assert_eq!(res, ContractError::PoolConfigNotFound {});

    let res = execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::CreatePool {
            pool_type: PoolType::Xyk {},
            asset_infos: asset_infos.clone(),
            init_params: None,
            total_fee_bps: None,
            staking_config: PartialStakeConfig::default(),
        },
    )
    .unwrap();

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "create_pair"),
            attr("pair", "asset0000-asset0001")
        ]
    );
    assert_eq!(
        res.messages,
        vec![SubMsg {
            msg: WasmMsg::Instantiate {
                msg: to_json_binary(&PoolInstantiateMsg {
                    factory_addr: String::from(MOCK_CONTRACT_ADDR),
                    asset_infos,
                    init_params: None,
                    staking_config: default_stake_config().to_stake_config(),
                    trading_starts: mock_env().block.time.seconds(),
                    fee_config: pair_config.fee_config,
                    circuit_breaker: None,
                })
                .unwrap(),
                code_id: pair_config.code_id,
                funds: vec![],
                admin: Some(config.unwrap().owner.to_string()),
                label: String::from("Dex pair"),
            }
            .into(),
            id: 1,
            gas_limit: None,
            reply_on: ReplyOn::Success
        }]
    );
}

#[test]
fn register() {
    let mut deps = mock_dependencies(&[]);
    let owner = "owner0000";

    let msg = InstantiateMsg {
        pool_configs: vec![PoolConfig {
            code_id: 123u64,
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 100,
                protocol_fee_bps: 10,
            },
            is_disabled: false,
        }],
        fee_address: None,
        owner: owner.to_string(),
        max_referral_commission: Decimal::one(),
        default_stake_config: default_stake_config(),
        trading_starts: None,
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let asset_infos = vec![
        AssetInfo::Cw20Token("asset0000".to_string()),
        AssetInfo::Cw20Token("asset0001".to_string()),
    ];

    let msg = ExecuteMsg::CreatePool {
        pool_type: PoolType::Xyk {},
        asset_infos: asset_infos.clone(),
        init_params: None,
        staking_config: PartialStakeConfig::default(),
        total_fee_bps: None,
    };

    let env = mock_env();
    let info = mock_info(owner, &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let pair0_addr = "pair0000".to_string();
    let validated_asset_infos: Vec<_> = asset_infos
        .iter()
        .cloned()
        .map(|a| a.validate(&deps.api).unwrap())
        .collect();
    let pair0_info = PairInfo {
        asset_infos: validated_asset_infos.clone(),
        contract_addr: Addr::unchecked("pair0000"),
        staking_addr: Addr::unchecked("stake0000"),
        liquidity_token: "liquidity0000".to_owned(),
        pool_type: PoolType::Xyk {},
        fee_config: FeeConfig {
            total_fee_bps: 0,
            protocol_fee_bps: 0,
        },
    };

    let mut deployed_pairs = vec![(&pair0_addr, &pair0_info)];

    // Register an Dex pair querier
    deps.querier.with_dex_pairs(&deployed_pairs);

    let instantiate_res = MsgInstantiateContractResponse {
        contract_address: String::from("pair0000"),
        data: None,
    };

    let _res = reply::instantiate_pair(deps.as_mut(), mock_env(), instantiate_res.clone()).unwrap();

    let query_res = query(
        deps.as_ref(),
        env,
        QueryMsg::Pool {
            asset_infos: asset_infos.clone(),
        },
    )
    .unwrap();

    let pair_res: PairInfo = from_json(&query_res).unwrap();
    assert_eq!(
        pair_res,
        PairInfo {
            liquidity_token: "liquidity0000".to_owned(),
            contract_addr: Addr::unchecked("pair0000"),
            staking_addr: Addr::unchecked("stake0000"),
            asset_infos: validated_asset_infos.clone(),
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 0,
                protocol_fee_bps: 0,
            },
        }
    );

    // Check pair was registered
    let res = reply::instantiate_pair(deps.as_mut(), mock_env(), instantiate_res).unwrap_err();
    assert_eq!(res, ContractError::PoolWasRegistered {});

    // Store one more item to test query pairs
    let asset_infos_2 = vec![
        AssetInfo::Cw20Token("asset0000".to_string()),
        AssetInfo::Cw20Token("asset0002".to_string()),
    ];
    let validated_asset_infos_2: Vec<_> = asset_infos_2
        .iter()
        .cloned()
        .map(|a| a.validate(&deps.api).unwrap())
        .collect();

    let msg = ExecuteMsg::CreatePool {
        pool_type: PoolType::Xyk {},
        asset_infos: asset_infos_2.clone(),
        init_params: None,
        staking_config: PartialStakeConfig::default(),
        total_fee_bps: None,
    };

    let env = mock_env();
    let info = mock_info(owner, &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let pair1_addr = "pair0001".to_string();
    let pair1_info = PairInfo {
        asset_infos: validated_asset_infos_2.clone(),
        contract_addr: Addr::unchecked("pair0001"),
        staking_addr: Addr::unchecked("stake0001"),
        liquidity_token: "liquidity0001".to_owned(),
        pool_type: PoolType::Xyk {},
        fee_config: FeeConfig {
            total_fee_bps: 0,
            protocol_fee_bps: 0,
        },
    };

    deployed_pairs.push((&pair1_addr, &pair1_info));

    // Register dex pair querier
    deps.querier.with_dex_pairs(&deployed_pairs);

    let instantiate_res = MsgInstantiateContractResponse {
        contract_address: String::from("pair0001"),
        data: None,
    };

    let _res = reply::instantiate_pair(deps.as_mut(), mock_env(), instantiate_res).unwrap();

    let query_msg = QueryMsg::Pools {
        start_after: None,
        limit: None,
    };

    let res = query(deps.as_ref(), env.clone(), query_msg).unwrap();
    let pairs_res: PoolsResponse = from_json(&res).unwrap();
    assert_eq!(
        pairs_res.pools,
        vec![
            PairInfo {
                liquidity_token: "liquidity0000".to_owned(),
                contract_addr: Addr::unchecked("pair0000"),
                staking_addr: Addr::unchecked("stake0000"),
                asset_infos: validated_asset_infos.clone(),
                pool_type: PoolType::Xyk {},
                fee_config: FeeConfig {
                    total_fee_bps: 0,
                    protocol_fee_bps: 0,
                },
            },
            PairInfo {
                liquidity_token: "liquidity0001".to_owned(),
                contract_addr: Addr::unchecked("pair0001"),
                staking_addr: Addr::unchecked("stake0001"),
                asset_infos: validated_asset_infos_2.clone(),
                pool_type: PoolType::Xyk {},
                fee_config: FeeConfig {
                    total_fee_bps: 0,
                    protocol_fee_bps: 0,
                },
            }
        ]
    );

    let query_msg = QueryMsg::Pools {
        start_after: None,
        limit: Some(1),
    };

    let res = query(deps.as_ref(), env.clone(), query_msg).unwrap();
    let pairs_res: PoolsResponse = from_json(&res).unwrap();
    assert_eq!(
        pairs_res.pools,
        vec![PairInfo {
            liquidity_token: "liquidity0000".to_owned(),
            contract_addr: Addr::unchecked("pair0000"),
            staking_addr: Addr::unchecked("stake0000"),
            asset_infos: validated_asset_infos.clone(),
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 0,
                protocol_fee_bps: 0,
            },
        }]
    );

    let query_msg = QueryMsg::Pools {
        start_after: Some(asset_infos),
        limit: None,
    };

    let res = query(deps.as_ref(), env, query_msg).unwrap();
    let pairs_res: PoolsResponse = from_json(&res).unwrap();
    assert_eq!(
        pairs_res.pools,
        vec![PairInfo {
            liquidity_token: "liquidity0001".to_owned(),
            contract_addr: Addr::unchecked("pair0001"),
            staking_addr: Addr::unchecked("stake0001"),
            asset_infos: validated_asset_infos_2,
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 0,
                protocol_fee_bps: 0,
            },
        }]
    );

    // Deregister from wrong acc
    let env = mock_env();
    let info = mock_info("wrong_addr0000", &[]);
    let res = execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::Deregister {
            asset_infos: asset_infos_2.clone(),
        },
    )
    .unwrap_err();

    assert_eq!(res, ContractError::Unauthorized {});

    // Proper deregister
    let env = mock_env();
    let info = mock_info(owner, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::Deregister {
            asset_infos: asset_infos_2,
        },
    )
    .unwrap();

    assert_eq!(res.attributes[0], attr("action", "deregister"));

    let query_msg = QueryMsg::Pools {
        start_after: None,
        limit: None,
    };

    let res = query(deps.as_ref(), env, query_msg).unwrap();
    let pairs_res: PoolsResponse = from_json(&res).unwrap();
    assert_eq!(
        pairs_res.pools,
        vec![PairInfo {
            liquidity_token: "liquidity0000".to_owned(),
            contract_addr: Addr::unchecked("pair0000"),
            staking_addr: Addr::unchecked("stake0000"),
            asset_infos: validated_asset_infos,
            pool_type: PoolType::Xyk {},
            fee_config: FeeConfig {
                total_fee_bps: 0,
                protocol_fee_bps: 0,
            },
        },]
    );
}
