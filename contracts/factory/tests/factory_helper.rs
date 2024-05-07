use anyhow::Result as AnyResult;

use bindings_test::CoreumApp;
use cosmwasm_std::{Addr, Binary, Coin, Decimal, Uint128};
use cw20::MinterResponse;
use cw_multi_test::{AppResponse, BankSudo, ContractWrapper, Executor, SudoMsg};

use dex::{
    asset::{Asset, AssetInfo},
    factory::{
        DefaultStakeConfig, PartialDefaultStakeConfig, PartialStakeConfig, PoolConfig, PoolType,
        QueryMsg,
    },
    fee_config::FeeConfig,
    pool::PairInfo,
};

pub struct FactoryHelper {
    pub owner: Addr,
    pub astro_token: Addr,
    pub factory: Addr,
    pub cw20_token_code_id: u64,
}

impl FactoryHelper {
    #[allow(dead_code)]
    pub fn init(router: &mut CoreumApp, owner: &Addr) -> Self {
        Self::instantiate(router, owner, None)
    }

    pub fn instantiate(router: &mut CoreumApp, owner: &Addr, factory_code_id: Option<u64>) -> Self {
        let base_token_cntract = Box::new(ContractWrapper::new_with_empty(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        ));

        let cw20_token_code_id = router.store_code(base_token_cntract);

        let msg = cw20_base::msg::InstantiateMsg {
            name: String::from("Base token"),
            symbol: String::from("BASE"),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: owner.to_string(),
                cap: None,
            }),
            marketing: None,
        };

        let astro_token = router
            .instantiate_contract(
                cw20_token_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("BASE"),
                None,
            )
            .unwrap();

        router
            .sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: astro_token.to_string(),
                amount: vec![Coin {
                    denom: "coreum".to_string(),
                    amount: Uint128::new(3_000),
                }],
            }))
            .unwrap();

        let pool_contract = Box::new(
            ContractWrapper::new(
                dex_pool::contract::execute,
                dex_pool::contract::instantiate,
                dex_pool::contract::query,
            )
            .with_reply(dex_pool::contract::reply),
        );

        let pool_code_id = router.store_code(pool_contract);

        let factory_code_id = if let Some(factory_code_id) = factory_code_id {
            factory_code_id
        } else {
            let factory_contract = Box::new(
                ContractWrapper::new(
                    dex_factory::contract::execute,
                    dex_factory::contract::instantiate,
                    dex_factory::contract::query,
                )
                .with_reply(dex_factory::contract::reply),
            );
            router.store_code(factory_contract)
        };

        let staking_contract = Box::new(ContractWrapper::new(
            dex_stake::contract::execute,
            dex_stake::contract::instantiate,
            dex_stake::contract::query,
        ));

        let staking_code_id = router.store_code(staking_contract);

        let msg = dex::factory::InstantiateMsg {
            pool_configs: vec![PoolConfig {
                code_id: pool_code_id,
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
            default_stake_config: DefaultStakeConfig {
                staking_code_id,
                tokens_per_power: Uint128::new(1000),
                min_bond: Uint128::new(1000),
                unbonding_periods: vec![1, 2, 3],
                max_distributions: 6,
            },
            trading_starts: None,
            permissionless_fee: Asset {
                info: AssetInfo::Cw20Token("coreum".to_string()),
                amount: Uint128::new(3_000),
            },
        };

        let factory = router
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("ASTRO"),
                Some(owner.to_string()),
            )
            .unwrap();

        Self {
            owner: owner.clone(),
            astro_token,
            factory,
            cw20_token_code_id,
        }
    }

    pub fn update_config(
        &mut self,
        router: &mut CoreumApp,
        sender: &Addr,
        fee_address: Option<String>,
        only_owner_can_create_pools: Option<bool>,
        default_stake_config: Option<PartialDefaultStakeConfig>,
    ) -> AnyResult<AppResponse> {
        let msg = dex::factory::ExecuteMsg::UpdateConfig {
            fee_address,
            only_owner_can_create_pools,
            default_stake_config,
        };

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])
    }

    pub fn create_pair(
        &mut self,
        router: &mut CoreumApp,
        sender: &Addr,
        pool_type: PoolType,
        tokens: [&str; 2],
        init_params: Option<Binary>,
        staking_config: Option<PartialStakeConfig>,
    ) -> AnyResult<AppResponse> {
        let asset_infos = vec![
            AssetInfo::SmartToken(tokens[0].to_owned()),
            AssetInfo::SmartToken(tokens[1].to_owned()),
        ];

        let msg = dex::factory::ExecuteMsg::CreatePool {
            pool_type,
            asset_infos,
            init_params,
            staking_config: staking_config.unwrap_or_default(),
            total_fee_bps: None,
        };

        router.execute_contract(
            sender.clone(),
            self.factory.clone(),
            &msg,
            &[Coin::new(3_000, "coreum")],
        )
    }

    #[allow(dead_code)]
    pub fn deregister_pool_and_staking(
        &mut self,
        router: &mut CoreumApp,
        sender: &Addr,
        asset_infos: Vec<AssetInfo>,
    ) -> AnyResult<AppResponse> {
        let msg = dex::factory::ExecuteMsg::Deregister { asset_infos };

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])
    }

    pub fn create_pair_with_addr(
        &mut self,
        router: &mut CoreumApp,
        sender: &Addr,
        pair_type: PoolType,
        tokens: [&str; 2],
        init_params: Option<Binary>,
    ) -> AnyResult<Addr> {
        self.create_pair(router, sender, pair_type, tokens, init_params, None)?;

        let asset_infos = vec![
            AssetInfo::SmartToken(tokens[0].to_owned()),
            AssetInfo::Cw20Token(tokens[1].to_owned()),
        ];

        let res: PairInfo = router
            .wrap()
            .query_wasm_smart(self.factory.clone(), &QueryMsg::Pool { asset_infos })?;

        Ok(res.contract_addr)
    }

    #[allow(dead_code)]
    pub fn update_pair_fees(
        &mut self,
        router: &mut CoreumApp,
        sender: &Addr,
        asset_infos: Vec<AssetInfo>,
        fee_config: FeeConfig,
    ) -> AnyResult<AppResponse> {
        let msg = dex::factory::ExecuteMsg::UpdatePoolFees {
            asset_infos,
            fee_config,
        };

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])
    }
}

pub fn instantiate_token(
    app: &mut CoreumApp,
    token_code_id: u64,
    owner: &Addr,
    token_name: &str,
    decimals: Option<u8>,
) -> Addr {
    let init_msg = cw20_base::msg::InstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: decimals.unwrap_or(6),
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    app.instantiate_contract(
        token_code_id,
        owner.clone(),
        &init_msg,
        &[],
        token_name,
        Some(owner.to_string()),
    )
    .unwrap()
}
