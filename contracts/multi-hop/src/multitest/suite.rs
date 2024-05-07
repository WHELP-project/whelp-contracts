use anyhow::Result as AnyResult;

use bindings_test::CoreumApp;
use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{coin, to_json_binary, Addr, Coin, Decimal, Uint128};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw20_base::msg::InstantiateMsg as Cw20BaseInstantiateMsg;
use cw_multi_test::{AppResponse, BankSudo, Contract, ContractWrapper, Executor, SudoMsg};

use crate::msg::{
    ExecuteMsg, InstantiateMsg, QueryMsg, SimulateSwapOperationsResponse, SwapOperation,
};
use dex::asset::{Asset, AssetInfo};
use dex::factory::{
    DefaultStakeConfig, ExecuteMsg as FactoryExecuteMsg, InstantiateMsg as FactoryInstantiateMsg,
    PoolConfig, PoolType, QueryMsg as FactoryQueryMsg,
};
use dex::fee_config::FeeConfig;
use dex::pool::{ExecuteMsg as PairExecuteMsg, PairInfo};

const SECONDS_PER_DAY: u64 = 60 * 60 * 24;

fn store_multi_hop(app: &mut CoreumApp) -> u64 {
    let contract: Box<dyn Contract<CoreumMsg, CoreumQueries>> = Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ));

    app.store_code(contract)
}

fn store_factory(app: &mut CoreumApp) -> u64 {
    let contract = Box::new(
        ContractWrapper::new(
            dex_factory::contract::execute,
            dex_factory::contract::instantiate,
            dex_factory::contract::query,
        )
        .with_reply(dex_factory::contract::reply),
    );

    app.store_code(contract)
}

fn store_pair(app: &mut CoreumApp) -> u64 {
    let contract: Box<dyn Contract<CoreumMsg, CoreumQueries>> = Box::new(
        ContractWrapper::new(
            dex_pool::contract::execute,
            dex_pool::contract::instantiate,
            dex_pool::contract::query,
        )
        .with_reply(dex_pool::contract::reply),
    );

    app.store_code(contract)
}

fn store_cw20(app: &mut CoreumApp) -> u64 {
    let contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));

    app.store_code(contract)
}

fn store_staking(app: &mut CoreumApp) -> u64 {
    let contract: Box<dyn Contract<CoreumMsg, CoreumQueries>> = Box::new(ContractWrapper::new(
        dex_stake::contract::execute,
        dex_stake::contract::instantiate,
        dex_stake::contract::query,
    ));

    app.store_code(contract)
}

#[derive(Debug)]
pub struct SuiteBuilder {
    funds: Vec<(Addr, Vec<Coin>)>,
    max_referral_commission: Decimal,
    stake_config: DefaultStakeConfig,
    total_fee_bps: u16,
    protocol_fee_bps: u16,
}

impl SuiteBuilder {
    pub fn new() -> Self {
        Self {
            funds: vec![],
            max_referral_commission: Decimal::one(),
            total_fee_bps: 0,
            protocol_fee_bps: 0,
            stake_config: DefaultStakeConfig {
                staking_code_id: 0, // will be set in build()
                tokens_per_power: Uint128::new(1000),
                min_bond: Uint128::new(1000),
                unbonding_periods: vec![
                    SECONDS_PER_DAY * 7,
                    SECONDS_PER_DAY * 14,
                    SECONDS_PER_DAY * 21,
                ],
                max_distributions: 6,
            },
        }
    }

    pub fn with_fees(mut self, total_fee_bps: u16, protocol_fee_bps: u16) -> Self {
        self.total_fee_bps = total_fee_bps;
        self.protocol_fee_bps = protocol_fee_bps;
        self
    }

    pub fn with_funds(mut self, addr: &str, funds: &[Coin]) -> Self {
        self.funds.push((Addr::unchecked(addr), funds.into()));
        self
    }

    pub fn with_max_referral_commission(mut self, max: Decimal) -> Self {
        self.max_referral_commission = max;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let mut app = CoreumApp::default();
        let owner = Addr::unchecked("owner");

        let cw20_code_id = store_cw20(&mut app);
        let pair_code_id = store_pair(&mut app);
        let factory_code_id = store_factory(&mut app);
        let staking_code_id = store_staking(&mut app);
        let factory = app
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &FactoryInstantiateMsg {
                    pool_configs: vec![
                        PoolConfig {
                            code_id: pair_code_id,
                            pool_type: PoolType::Xyk {},
                            fee_config: FeeConfig {
                                total_fee_bps: self.total_fee_bps,
                                protocol_fee_bps: self.protocol_fee_bps,
                            },
                            is_disabled: false,
                        },
                        PoolConfig {
                            code_id: pair_code_id,
                            pool_type: PoolType::Stable {},
                            fee_config: FeeConfig {
                                total_fee_bps: self.total_fee_bps,
                                protocol_fee_bps: self.protocol_fee_bps,
                            },
                            is_disabled: false,
                        },
                    ],
                    fee_address: None,
                    owner: owner.to_string(),
                    max_referral_commission: self.max_referral_commission,
                    default_stake_config: DefaultStakeConfig {
                        staking_code_id,
                        ..self.stake_config
                    },
                    trading_starts: None,
                    permissionless_fee: Asset {
                        info: AssetInfo::Cw20Token("coreum".to_string()),
                        amount: Uint128::new(3_000),
                    },
                },
                &[],
                "Dex Factory",
                None,
            )
            .unwrap();

        let multi_hop_code_id = store_multi_hop(&mut app);
        let multi_hop = app
            .instantiate_contract(
                multi_hop_code_id,
                owner.clone(),
                &InstantiateMsg {
                    dex_factory: factory.to_string(),
                },
                &[],
                "Multi Hop",
                None,
            )
            .unwrap();

        let funds = self.funds;
        app.init_modules(|router, _, storage| -> AnyResult<()> {
            for (addr, coin) in funds {
                router.bank.init_balance(storage, &addr, coin)?;
            }
            Ok(())
        })
        .unwrap();

        Suite {
            owner: owner.to_string(),
            app,
            factory,
            multi_hop,
            cw20_code_id,
        }
    }
}

pub struct Suite {
    pub owner: String,
    pub app: CoreumApp,
    pub factory: Addr,
    multi_hop: Addr,
    cw20_code_id: u64,
}

impl Suite {
    fn create_pair(
        &mut self,
        sender: &str,
        pool_type: PoolType,
        tokens: [AssetInfo; 2],
    ) -> AnyResult<Addr> {
        self.app
            .init_modules(|router, _, storage| {
                router.bank.init_balance(
                    storage,
                    &Addr::unchecked(sender),
                    vec![Coin {
                        denom: "coreum".to_string(),
                        amount: Uint128::new(6_000),
                    }],
                )
            })
            .unwrap();

        self.app.execute_contract(
            Addr::unchecked(sender),
            self.factory.clone(),
            &FactoryExecuteMsg::CreatePool {
                pool_type,
                asset_infos: tokens.to_vec(),
                init_params: None,
                staking_config: Default::default(),
                total_fee_bps: None,
            },
            &[Coin::new(3_000, "coreum")],
        )?;

        let factory = self.factory.clone();
        let res: PairInfo = self.app.wrap().query_wasm_smart(
            Addr::unchecked(factory),
            &FactoryQueryMsg::Pool {
                asset_infos: tokens.to_vec(),
            },
        )?;
        Ok(res.contract_addr)
    }

    fn provide_liquidity(
        &mut self,
        owner: &str,
        pair: &Addr,
        assets: [Asset; 2],
        send_funds: &[Coin],
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(owner),
            pair.clone(),
            &PairExecuteMsg::ProvideLiquidity {
                assets: assets.to_vec(),
                slippage_tolerance: None,
                receiver: None,
            },
            send_funds,
        )
    }

    fn increase_allowance(
        &mut self,
        owner: &str,
        contract: &Addr,
        spender: &str,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(owner),
            contract.clone(),
            &Cw20ExecuteMsg::IncreaseAllowance {
                spender: spender.to_owned(),
                amount: amount.into(),
                expires: None,
            },
            &[],
        )
    }

    /// Create LP for provided assets and provides some liquidity to them.
    /// Requirement: if using native token provide coins to sent as last argument
    pub fn create_pair_and_provide_liquidity(
        &mut self,
        pool_type: PoolType,
        first_asset: (AssetInfo, u128),
        second_asset: (AssetInfo, u128),
        native_tokens: Vec<Coin>,
    ) -> AnyResult<Addr> {
        let owner = self.owner.clone();
        let whale = "whale";

        let pair = self.create_pair(
            &owner,
            pool_type,
            [first_asset.0.clone(), second_asset.0.clone()],
        )?;

        match first_asset.0.clone() {
            AssetInfo::Cw20Token(addr) => {
                // Mint some initial balances for whale user
                self.mint_cw20(&owner, &Addr::unchecked(&addr), first_asset.1, whale)
                    .unwrap();
                // Increases allowances for given LP contracts in order to provide liquidity to pool
                self.increase_allowance(
                    whale,
                    &Addr::unchecked(addr),
                    pair.as_str(),
                    first_asset.1,
                )
                .unwrap();
            }
            AssetInfo::SmartToken(denom) => {
                self.app
                    .sudo(SudoMsg::Bank(BankSudo::Mint {
                        to_address: whale.to_owned(),
                        amount: vec![coin(first_asset.1, denom)],
                    }))
                    .unwrap();
            }
        };
        match second_asset.0.clone() {
            AssetInfo::Cw20Token(addr) => {
                // Mint some initial balances for whale user
                self.mint_cw20(&owner, &Addr::unchecked(&addr), second_asset.1, whale)
                    .unwrap();
                // Increases allowances for given LP contracts in order to provide liquidity to pool
                self.increase_allowance(
                    whale,
                    &Addr::unchecked(addr),
                    pair.as_str(),
                    second_asset.1,
                )
                .unwrap();
            }
            AssetInfo::SmartToken(denom) => {
                self.app
                    .sudo(SudoMsg::Bank(BankSudo::Mint {
                        to_address: whale.to_owned(),
                        amount: vec![coin(second_asset.1, denom)],
                    }))
                    .unwrap();
            }
        };

        self.provide_liquidity(
            whale,
            &pair,
            [
                Asset {
                    info: first_asset.0,
                    amount: first_asset.1.into(),
                },
                Asset {
                    info: second_asset.0,
                    amount: second_asset.1.into(),
                },
            ],
            &native_tokens, // for native token you need to transfer tokens manually
        )
        .unwrap();

        Ok(pair)
    }

    pub fn instantiate_token(&mut self, owner: &str, token: &str) -> Addr {
        self.app
            .instantiate_contract(
                self.cw20_code_id,
                Addr::unchecked(owner),
                &Cw20BaseInstantiateMsg {
                    name: "Cra".to_owned(),
                    symbol: "CRA".to_owned(),
                    decimals: 6,
                    initial_balances: vec![],
                    mint: Some(MinterResponse {
                        minter: owner.to_string(),
                        cap: None,
                    }),
                    marketing: None,
                },
                &[],
                token,
                None,
            )
            .unwrap()
    }

    pub fn mint_cw20(
        &mut self,
        owner: &str,
        token: &Addr,
        amount: u128,
        recipient: &str,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(owner),
            token.clone(),
            &Cw20ExecuteMsg::Mint {
                recipient: recipient.to_owned(),
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn swap_operations(
        &mut self,
        sender: &str,
        amount: Coin,
        operations: Vec<SwapOperation>,
    ) -> AnyResult<AppResponse> {
        self.swap_operations_ref(sender, amount, operations, None, None)
    }

    pub fn swap_operations_ref(
        &mut self,
        sender: &str,
        amount: Coin,
        operations: Vec<SwapOperation>,
        referral_address: impl Into<Option<String>>,
        referral_commission: impl Into<Option<Decimal>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.multi_hop.clone(),
            &ExecuteMsg::ExecuteSwapOperations {
                operations,
                minimum_receive: None,
                receiver: None,
                max_spread: None,
                referral_address: referral_address.into(),
                referral_commission: referral_commission.into(),
            },
            &[amount],
        )
    }

    pub fn swap_operations_cw20(
        &mut self,
        sender: &str,
        token_in: &Addr,
        amount: u128,
        operations: Vec<SwapOperation>,
    ) -> AnyResult<AppResponse> {
        self.swap_operations_cw20_ref(sender, token_in, amount, operations, None, None)
    }

    pub fn swap_operations_cw20_ref(
        &mut self,
        sender: &str,
        token_in: &Addr,
        amount: u128,
        operations: Vec<SwapOperation>,
        referral_address: impl Into<Option<String>>,
        referral_commission: impl Into<Option<Decimal>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            token_in.clone(),
            &Cw20ExecuteMsg::Send {
                contract: self.multi_hop.to_string(),
                amount: amount.into(),
                msg: to_json_binary(&ExecuteMsg::ExecuteSwapOperations {
                    operations,
                    minimum_receive: None,
                    receiver: None,
                    max_spread: None,
                    referral_address: referral_address.into(),
                    referral_commission: referral_commission.into(),
                })
                .unwrap(),
            },
            &[],
        )
    }

    pub fn assert_minimum_receive(
        &mut self,
        receiver: &str,
        asset_info: AssetInfo,
        minimum_receive: impl Into<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(receiver),
            self.multi_hop.clone(),
            &ExecuteMsg::AssertMinimumReceive {
                asset_info,
                prev_balance: Uint128::zero(),
                minimum_receive: minimum_receive.into(),
                receiver: receiver.into(),
            },
            &[],
        )
    }

    pub fn query_balance(&self, sender: &str, denom: &str) -> AnyResult<u128> {
        let amount = self
            .app
            .wrap()
            .query_balance(Addr::unchecked(sender), denom)?
            .amount;
        Ok(amount.into())
    }

    pub fn query_cw20_balance(&self, sender: &str, address: &Addr) -> AnyResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            address,
            &Cw20QueryMsg::Balance {
                address: sender.to_owned(),
            },
        )?;
        Ok(balance.balance.into())
    }

    pub fn query_simulate_swap_operations(
        &self,
        offer_amount: impl Into<Uint128>,
        operations: Vec<SwapOperation>,
    ) -> AnyResult<SimulateSwapOperationsResponse> {
        let res: SimulateSwapOperationsResponse = self.app.wrap().query_wasm_smart(
            self.multi_hop.clone(),
            &QueryMsg::SimulateSwapOperations {
                offer_amount: offer_amount.into(),
                operations,
                referral: false,
                referral_commission: None,
            },
        )?;
        Ok(res)
    }

    pub fn query_simulate_swap_operations_ref(
        &self,
        offer_amount: impl Into<Uint128>,
        operations: Vec<SwapOperation>,
        referral_commission: impl Into<Option<Decimal>>,
    ) -> AnyResult<SimulateSwapOperationsResponse> {
        let res: SimulateSwapOperationsResponse = self.app.wrap().query_wasm_smart(
            self.multi_hop.clone(),
            &QueryMsg::SimulateSwapOperations {
                offer_amount: offer_amount.into(),
                operations,
                referral: true,
                referral_commission: referral_commission.into(),
            },
        )?;
        Ok(res)
    }

    pub fn query_simulate_reverse_swap_operations(
        &self,
        ask_amount: impl Into<Uint128>,
        operations: Vec<SwapOperation>,
    ) -> AnyResult<SimulateSwapOperationsResponse> {
        let res: SimulateSwapOperationsResponse = self.app.wrap().query_wasm_smart(
            self.multi_hop.clone(),
            &QueryMsg::SimulateReverseSwapOperations {
                ask_amount: ask_amount.into(),
                operations,
                referral: false,
                referral_commission: None,
            },
        )?;
        Ok(res)
    }

    pub fn query_simulate_reverse_swap_operations_ref(
        &self,
        ask_amount: impl Into<Uint128>,
        operations: Vec<SwapOperation>,
        referral_commission: impl Into<Option<Decimal>>,
    ) -> AnyResult<SimulateSwapOperationsResponse> {
        let res: SimulateSwapOperationsResponse = self.app.wrap().query_wasm_smart(
            self.multi_hop.clone(),
            &QueryMsg::SimulateReverseSwapOperations {
                ask_amount: ask_amount.into(),
                operations,
                referral: true,
                referral_commission: referral_commission.into(),
            },
        )?;
        Ok(res)
    }
}
