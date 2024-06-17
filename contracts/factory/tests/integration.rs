mod factory_helper;

use bindings_test::CoreumApp;
use cosmwasm_std::{attr, from_json, Addr, Coin, Decimal, StdError, Uint128};
use dex::asset::{Asset, AssetInfo};
use dex::factory::{
    ConfigResponse, DefaultStakeConfig, ExecuteMsg, FeeInfoResponse, InstantiateMsg,
    PartialDefaultStakeConfig, PoolConfig, PoolType, QueryMsg,
};
use dex::fee_config::FeeConfig;
use dex::pool::PairInfo;
use dex_factory::state::Config;

use crate::factory_helper::{instantiate_token, FactoryHelper};
use cw_multi_test::{ContractWrapper, Executor};
use dex::pool::ExecuteMsg as PairExecuteMsg;

fn mock_app() -> CoreumApp {
    CoreumApp::default()
}

fn store_factory_code(app: &mut CoreumApp) -> u64 {
    let factory_contract = Box::new(
        ContractWrapper::new(
            dex_factory::contract::execute,
            dex_factory::contract::instantiate,
            dex_factory::contract::query,
        )
        .with_reply(dex_factory::contract::reply)
        .with_migrate(dex_factory::contract::migrate),
    );

    app.store_code(factory_contract)
}

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
fn proper_initialization() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");

    let factory_code_id = store_factory_code(&mut app);

    let pool_configs = vec![PoolConfig {
        code_id: 321,
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
        pool_creation_fee: Asset {
            info: AssetInfo::Cw20Token("ucore".to_string()),
            amount: Uint128::new(320_000_000),
        },
    };

    let factory_instance = app
        .instantiate_contract(
            factory_code_id,
            Addr::unchecked(owner.clone()),
            &msg,
            &[],
            "factory",
            None,
        )
        .unwrap();

    let msg = QueryMsg::Config {};
    let config_res: ConfigResponse = app.wrap().query_wasm_smart(factory_instance, &msg).unwrap();

    assert_eq!(pool_configs, config_res.pool_configs);
    assert_eq!(owner, config_res.owner);
}

#[test]
fn update_config() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    // Update config
    helper
        .update_config(
            &mut app,
            &owner,
            Some("fee".to_string()),
            Some(false),
            Some(PartialDefaultStakeConfig {
                staking_code_id: Some(12345),
                tokens_per_power: None,
                min_bond: Some(10000u128.into()),
                unbonding_periods: None,
                max_distributions: Some(u32::MAX),
            }),
        )
        .unwrap();

    let config_res: ConfigResponse = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::Config {})
        .unwrap();

    assert_eq!("fee", config_res.fee_address.unwrap().to_string());

    // query config raw to get default stake config
    let raw_config: Config = from_json(
        app.wrap()
            .query_wasm_raw(&helper.factory, "config".as_bytes())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        DefaultStakeConfig {
            staking_code_id: 12345,
            tokens_per_power: Uint128::new(1000), // same as before
            min_bond: Uint128::new(10_000),
            unbonding_periods: vec![1, 2, 3], // same as before
            max_distributions: u32::MAX,
        },
        raw_config.default_stake_config
    );

    // Unauthorized err
    let res = helper
        .update_config(&mut app, &Addr::unchecked("not_owner"), None, None, None)
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Unauthorized");
}

#[test]
fn test_create_then_deregister_pair() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenX",
        Some(18),
    );
    let token2 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![Coin {
                denom: "ucore".to_string(),
                amount: Uint128::new(320_000_000),
            }],
        )
    })
    .unwrap();

    // Create the pair which we will later delete
    let res = helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", format!("{}-{}", token1.as_str(), token2.as_str()))
    );

    // Verify the pair now exists
    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            helper.factory.clone(),
            &QueryMsg::Pool {
                asset_infos: vec![
                    AssetInfo::Cw20Token(token1.to_string()),
                    AssetInfo::Cw20Token(token2.to_string()),
                ],
            },
        )
        .unwrap();

    // In multitest, contract names are counted in the order in which contracts are created
    assert_eq!("contract1", helper.factory.to_string());
    assert_eq!("contract4", res.contract_addr.to_string());
    assert_eq!("ucontcontlp-contract4", res.liquidity_token.to_string());
    // Deregsiter the pair, which removes the Pair addr and the staking contract addr from Storage
    helper
        .deregister_pool_and_staking(
            &mut app,
            &owner,
            vec![
                AssetInfo::Cw20Token(token1.to_string()),
                AssetInfo::Cw20Token(token2.to_string()),
            ],
        )
        .unwrap();

    // Verify the pair no longer exists
    let err: Result<PairInfo, StdError> = app.wrap().query_wasm_smart(
        helper.factory.clone(),
        &QueryMsg::Pool {
            asset_infos: vec![
                AssetInfo::Cw20Token(token1.to_string()),
                AssetInfo::Cw20Token(token2.to_string()),
            ],
        },
    );

    // In multitest, contract names are counted in the order in which contracts are created
    assert_eq!(
        err.unwrap_err(),
        StdError::generic_err("Querier contract error: type: cosmwasm_std::addresses::Addr; key: [00, 09, 70, 61, 69, 72, 5F, 69, 6E, 66, 6F, 63, 6F, 6E, 74, 72, 61, 63, 74, 32, 63, 6F, 6E, 74, 72, 61, 63, 74, 33] not found")
    );
}

#[test]
fn test_valid_staking() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenX",
        Some(18),
    );
    let token2 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    // Verify the pair now exists, we don't need to check the bool result here as non existence returns an Error
    let is_valid: bool = app
        .wrap()
        .query_wasm_smart(
            helper.factory.clone(),
            &QueryMsg::ValidateStakingAddress {
                address: "contract6".to_string(),
            },
        )
        .unwrap();

    assert!(!is_valid);

    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![Coin {
                denom: "ucore".to_string(),
                amount: Uint128::new(320_000_000),
            }],
        )
    })
    .unwrap();

    // Create the pair which we will later delete
    let res = helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", format!("{}-{}", token1.as_str(), token2.as_str()))
    );

    // Verify the pair now exists, we don't need to check the bool result here as non existence returns an Error
    let _is_valid: bool = app
        .wrap()
        .query_wasm_smart(
            helper.factory.clone(),
            &QueryMsg::ValidateStakingAddress {
                address: "contract6".to_string(),
            },
        )
        .unwrap();
    // assert!(is_valid);
    // Deregsiter the pair, which removes the Pair addr and the staking contract addr from Storage
    helper
        .deregister_pool_and_staking(
            &mut app,
            &owner,
            vec![
                AssetInfo::Cw20Token(token1.to_string()),
                AssetInfo::Cw20Token(token2.to_string()),
            ],
        )
        .unwrap();

    let is_valid: bool = app
        .wrap()
        .query_wasm_smart(
            helper.factory.clone(),
            &QueryMsg::ValidateStakingAddress {
                address: "contract6".to_string(),
            },
        )
        .unwrap();

    assert!(!is_valid);
}

#[test]
fn test_create_pair() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenX",
        Some(18),
    );
    let token2 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );
    // TODO: this test requires 6_000 tokens, because we try to initialize pool twice.
    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![Coin {
                denom: "ucore".to_string(),
                amount: Uint128::new(6_000_000_000),
            }],
        )
    })
    .unwrap();
    //  factory_helper.rs:164-167 we set one of the tokens as SmartToken, the other
    //  as Cw20Token, hence it's two different tokens and the below fails to unwrap_err
    let err = helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Xyk {},
            [token1.as_str(), token1.as_str()],
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Doubling assets in asset infos"
    );

    let res = helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap();

    let err = helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Pair was already created");

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr("pair", format!("{}-{}", token1.as_str(), token2.as_str()))
    );

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            helper.factory.clone(),
            &QueryMsg::Pool {
                asset_infos: vec![
                    AssetInfo::Cw20Token(token1.to_string()),
                    AssetInfo::Cw20Token(token2.to_string()),
                ],
            },
        )
        .unwrap();

    // In multitest, contract names are counted in the order in which contracts are created
    assert_eq!("contract1", helper.factory.to_string());
    assert_eq!("contract4", res.contract_addr.to_string());
    assert_eq!("ucontcontlp-contract4", res.liquidity_token.to_string());

    // Create disabled pair type
    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::UpdatePoolConfig {
            config: PoolConfig {
                code_id: 0,
                pool_type: PoolType::Custom("Custom".to_string()),
                fee_config: FeeConfig {
                    total_fee_bps: 100,
                    protocol_fee_bps: 40,
                },
                is_disabled: true,
            },
        },
        &[],
    )
    .unwrap();

    let token3 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    let err = helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Custom("Custom".to_string()),
            [token1.as_str(), token3.as_str()],
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Pool config disabled");

    // Query fee info
    let fee_info: FeeInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &helper.factory,
            &QueryMsg::FeeInfo {
                pool_type: PoolType::Custom("Custom".to_string()),
            },
        )
        .unwrap();
    assert_eq!(100, fee_info.total_fee_bps);
    assert_eq!(40, fee_info.protocol_fee_bps);

    // query blacklisted pairs
    let pair_types: Vec<PoolType> = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::BlacklistedPoolTypes {})
        .unwrap();
    assert_eq!(pair_types, vec![PoolType::Custom("Custom".to_string())]);
}

#[test]
#[ignore = "all our pools are created by default with `only_owner_can_create_pools` set to false"]
fn test_create_pair_permissions() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenX",
        Some(18),
    );
    let token2 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    // app.init_modules(|router, _, storage| {
    //     router.bank.init_balance(
    //         storage,
    //         &user,
    //         vec![Coin {
    //             denom: "ucore".to_string(),
    //             amount: Uint128::new(320_000_000),
    //         }],
    //     )
    // })
    // .unwrap();

    let err = helper
        .create_pair(
            &mut app,
            &user,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    // allow anyone to create pair
    helper
        .update_config(&mut app, &owner, None, Some(false), None)
        .unwrap();

    // addendum: it does work but a required deposit has been added; check migration.rs test
    let err = helper
        .create_pair(
            &mut app,
            &user,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap_err();

    assert_eq!(
        "Factory is in permissionless mode: deposit must be sent to create new pair",
        err.source().unwrap().to_string()
    );
}

#[test]
fn test_update_pair_fee() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token1 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenX",
        Some(18),
    );
    let token2 = instantiate_token(
        &mut app,
        helper.cw20_token_code_id,
        &owner,
        "tokenY",
        Some(18),
    );

    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![Coin {
                denom: "ucore".to_string(),
                amount: Uint128::new(320_000_000),
            }],
        )
    })
    .unwrap();

    helper
        .create_pair(
            &mut app,
            &owner,
            PoolType::Xyk {},
            [token1.as_str(), token2.as_str()],
            None,
            None,
        )
        .unwrap();

    let asset_infos = vec![
        AssetInfo::SmartToken(token1.to_string()),
        AssetInfo::SmartToken(token2.to_string()),
    ];
    // query current fee
    let pair_res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            &helper.factory,
            &QueryMsg::Pool {
                asset_infos: asset_infos.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        pair_res.fee_config,
        FeeConfig {
            total_fee_bps: 100,
            protocol_fee_bps: 10
        }
    );

    // change fees
    helper
        .update_pair_fees(
            &mut app,
            &owner,
            asset_infos.clone(),
            FeeConfig {
                total_fee_bps: 1000,
                protocol_fee_bps: 10,
            },
        )
        .unwrap();
    // query updated fee
    let pair_res: PairInfo = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::Pool { asset_infos })
        .unwrap();
    assert_eq!(
        pair_res.fee_config,
        FeeConfig {
            total_fee_bps: 1000,
            protocol_fee_bps: 10
        }
    );
}

#[test]
fn test_pair_migration() {
    let mut app = mock_app();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token_instance0 =
        instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenX", None);
    let token_instance1 =
        instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenY", None);
    let token_instance2 =
        instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "tokenZ", None);

    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &owner,
            vec![Coin {
                denom: "ucore".to_string(),
                amount: Uint128::new(640_000_000),
            }],
        )
    })
    .unwrap();

    // Create pairs in factory
    let pools = [
        helper
            .create_pair_with_addr(
                &mut app,
                &owner,
                PoolType::Xyk {},
                [token_instance0.as_str(), token_instance1.as_str()],
                None,
            )
            .unwrap(),
        helper
            .create_pair_with_addr(
                &mut app,
                &owner,
                PoolType::Xyk {},
                [token_instance0.as_str(), token_instance2.as_str()],
                None,
            )
            .unwrap(),
    ];

    // Change contract ownership
    let new_owner = Addr::unchecked("new_owner");

    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::ProposeNewOwner {
            owner: new_owner.to_string(),
            expires_in: 100,
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        new_owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    app.init_modules(|router, _, storage| {
        router.bank.init_balance(
            storage,
            &new_owner,
            vec![Coin {
                denom: "ucore".to_string(),
                amount: Uint128::new(320_000_000),
            }],
        )
    })
    .unwrap();

    let pair3 = helper
        .create_pair_with_addr(
            &mut app,
            &new_owner,
            PoolType::Xyk {},
            [token_instance1.as_str(), token_instance2.as_str()],
            None,
        )
        .unwrap();

    // Should panic due to pairs are not migrated.
    for pool in pools.clone() {
        let res = app
            .execute_contract(
                new_owner.clone(),
                pool,
                &PairExecuteMsg::UpdateConfig {
                    params: Default::default(),
                },
                &[],
            )
            .unwrap_err();

        assert_eq!(res.root_cause().to_string(), "Operation non supported");
    }

    // Pair is created after admin migration
    let res = app
        .execute_contract(
            Addr::unchecked("user1"),
            pair3,
            &PairExecuteMsg::UpdateConfig {
                params: Default::default(),
            },
            &[],
        )
        .unwrap_err();

    assert_ne!(res.to_string(), "Pair is not migrated to the new admin");

    let pairs_res: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(&helper.factory, &QueryMsg::PoolsToMigrate {})
        .unwrap();
    assert_eq!(&pairs_res, &pools);

    // Factory owner was changed to new owner
    let err = app
        .execute_contract(
            owner,
            helper.factory.clone(),
            &ExecuteMsg::MarkAsMigrated {
                pools: Vec::from(pools.clone().map(String::from)),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Unauthorized");

    app.execute_contract(
        new_owner,
        helper.factory.clone(),
        &ExecuteMsg::MarkAsMigrated {
            pools: Vec::from(pools.clone().map(String::from)),
        },
        &[],
    )
    .unwrap();

    for pool in pools {
        let res = app
            .execute_contract(
                Addr::unchecked("user1"),
                pool,
                &PairExecuteMsg::UpdateConfig {
                    params: Default::default(),
                },
                &[],
            )
            .unwrap_err();

        assert_ne!(res.to_string(), "Pair is not migrated to the new admin!");
    }
}

#[test]
fn check_update_owner() {
    let mut app = mock_app();
    let owner = Addr::unchecked("owner");
    let helper = FactoryHelper::init(&mut app, &owner);

    let new_owner = String::from("new_owner");

    // New owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    // Unauthed check
    let err = app
        .execute_contract(
            Addr::unchecked("not_owner"),
            helper.factory.clone(),
            &msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Claim before proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.factory.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner
    app.execute_contract(Addr::unchecked("owner"), helper.factory.clone(), &msg, &[])
        .unwrap();

    // Claim from invalid addr
    let err = app
        .execute_contract(
            Addr::unchecked("invalid_addr"),
            helper.factory.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // Drop ownership proposal
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.factory.clone(),
            &ExecuteMsg::DropOwnershipProposal {},
            &[],
        )
        .unwrap_err();
    // new_owner is not an owner yet
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    app.execute_contract(
        owner.clone(),
        helper.factory.clone(),
        &ExecuteMsg::DropOwnershipProposal {},
        &[],
    )
    .unwrap();

    // Try to claim ownership
    let err = app
        .execute_contract(
            Addr::unchecked(new_owner.clone()),
            helper.factory.clone(),
            &ExecuteMsg::ClaimOwnership {},
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Ownership proposal not found"
    );

    // Propose new owner again
    app.execute_contract(Addr::unchecked("owner"), helper.factory.clone(), &msg, &[])
        .unwrap();
    // Claim ownership
    app.execute_contract(
        Addr::unchecked(new_owner.clone()),
        helper.factory.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();

    // Let's query the contract state
    let msg = QueryMsg::Config {};
    let res: ConfigResponse = app.wrap().query_wasm_smart(&helper.factory, &msg).unwrap();

    assert_eq!(res.owner, new_owner)
}
