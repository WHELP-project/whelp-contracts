use std::collections::HashMap;

use anyhow::{bail, Result as AnyResult};

use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{coin, to_json_binary, Addr, Coin, Decimal, StdResult, Uint128};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_controllers::{Claim, ClaimsResponse};
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};
use dex::{
    asset::{AssetInfo, AssetInfoExt, AssetInfoValidated, AssetValidated},
    stake::{InstantiateMsg, UnbondingPeriod, FundingInfo, ReceiveMsg},
};
use bindings_test::CoreumApp;

use crate::msg::{
    AllStakedResponse, AnnualizedReward, AnnualizedRewardsResponse, BondingInfoResponse,
    BondingPeriodInfo, DelegatedResponse, DistributedRewardsResponse, ExecuteMsg, QueryMsg,
    RewardsPowerResponse, StakedResponse, TotalStakedResponse, UnbondAllResponse,
    UndistributedRewardsResponse, WithdrawableRewardsResponse,
};

pub const SEVEN_DAYS: u64 = 604800;

fn contract_stake() -> Box<dyn Contract<CoreumMsg, CoreumQueries>> {
    let contract = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );

    Box::new(contract)
}

pub const COREUM_DENOM: &str = "ucore";

pub(super) fn juno_power(amount: u128) -> Vec<(AssetInfoValidated, u128)> {
    vec![(
        AssetInfoValidated::SmartToken(COREUM_DENOM.to_string()),
        amount,
    )]
}

pub(super) fn juno(amount: u128) -> AssetValidated {
    AssetInfoValidated::SmartToken(COREUM_DENOM.to_string()).with_balance(amount)
}

pub(super) fn native_token(denom: String, amount: u128) -> AssetValidated {
    AssetInfoValidated::SmartToken(denom).with_balance(amount)
}

#[derive(Debug)]
pub struct SuiteBuilder {
    pub lp_share_denom: String,
    pub tokens_per_power: Uint128,
    pub min_bond: Uint128,
    pub unbonding_periods: Vec<UnbondingPeriod>,
    pub admin: Option<String>,
    pub unbonder: Option<String>,
    pub native_balances: Vec<(Addr, Coin)>,
}

impl SuiteBuilder {
    pub fn new() -> Self {
        Self {
            lp_share_denom: "".to_owned(),
            tokens_per_power: Uint128::new(1000),
            min_bond: Uint128::new(5000),
            unbonding_periods: vec![SEVEN_DAYS],
            admin: None,
            unbonder: None,
            native_balances: vec![],
        }
    }

    pub fn with_lp_share_denom(mut self, denom: String) -> Self {
        self.lp_share_denom = denom;
        self
    }

    pub fn with_native_balances(mut self, denom: &str, balances: Vec<(&str, u128)>) -> Self {
        self.native_balances
            .extend(balances.into_iter().map(|(addr, amount)| {
                (
                    Addr::unchecked(addr),
                    Coin {
                        denom: denom.to_owned(),
                        amount: amount.into(),
                    },
                )
            }));
        self
    }

    pub fn with_min_bond(mut self, min_bond: u128) -> Self {
        self.min_bond = min_bond.into();
        self
    }

    pub fn with_tokens_per_power(mut self, tokens_per_power: u128) -> Self {
        self.tokens_per_power = tokens_per_power.into();
        self
    }

    pub fn with_admin(mut self, admin: &str) -> Self {
        self.admin = Some(admin.to_owned());
        self
    }

    pub fn with_unbonder(mut self, unbonder: &str) -> Self {
        self.unbonder = Some(unbonder.to_owned());
        self
    }

    pub fn with_unbonding_periods(mut self, unbonding_periods: Vec<UnbondingPeriod>) -> Self {
        self.unbonding_periods = unbonding_periods;
        self
    }

    #[track_caller]
    pub fn build(self) -> Suite {
        let mut app: CoreumApp = CoreumApp::new();
        // provide initial native balances
        app.init_modules(|router, _, storage| {
            // group by address
            let mut balances = HashMap::<Addr, Vec<Coin>>::new();
            for (addr, coin) in self.native_balances {
                let addr_balance = balances.entry(addr).or_default();
                addr_balance.push(coin);
            }

            for (addr, coins) in balances {
                router
                    .bank
                    .init_balance(storage, &addr, coins)
                    .expect("init balance");
            }
        });

        let admin = Addr::unchecked("admin");

        let stake_id = app.store_code(contract_stake());
        let stake_contract = app
            .instantiate_contract(
                stake_id,
                admin,
                &InstantiateMsg {
                    lp_share_denom: self.lp_share_denom,
                    tokens_per_power: self.tokens_per_power,
                    min_bond: self.min_bond,
                    unbonding_periods: self.unbonding_periods,
                    admin: self.admin,
                    unbonder: self.unbonder,
                    max_distributions: 6,
                },
                &[],
                "stake",
                None,
            )
            .unwrap();

        Suite {
            app,
            stake_contract,
            lp_share: self.lp_share_denom,
        }
    }
}

pub struct Suite {
    pub app: CoreumTestApp,
    stake_contract: Addr,
    lp_share: String,
}

impl Suite {
    pub fn stake_contract(&self) -> String {
        self.stake_contract.to_string()
    }

    // update block's time to simulate passage of time
    pub fn update_time(&mut self, time_update: u64) {
        let mut block = self.app.block_info();
        block.time = block.time.plus_seconds(time_update);
        self.app.set_block(block);
    }

    fn unbonding_period_or_default(&self, unbonding_period: impl Into<Option<u64>>) -> u64 {
        // Use default SEVEN_DAYS unbonding period if none provided
        if let Some(up) = unbonding_period.into() {
            up
        } else {
            SEVEN_DAYS
        }
    }

    // create a new distribution flow for staking
    pub fn create_distribution_flow(
        &mut self,
        sender: &str,
        manager: &str,
        asset: AssetInfo,
        rewards: Vec<(UnbondingPeriod, Decimal)>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::CreateDistributionFlow {
                manager: manager.to_string(),
                asset,
                rewards,
            },
            &[],
        )
    }

    // call to staking contract by sender
    pub fn delegate(
        &mut self,
        sender: &str,
        amount: u128,
        unbonding_period: impl Into<Option<u64>>,
    ) -> AnyResult<AppResponse> {
        self.delegate_as(sender, amount, unbonding_period, None)
    }

    // call to staking contract by sender
    pub fn delegate_as(
        &mut self,
        sender: &str,
        amount: u128,
        unbonding_period: impl Into<Option<u64>>,
        delegate_as: Option<&str>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::Delegate {
                unbonding_period: self.unbonding_period_or_default(unbonding_period),
            },
            &[coin(amount, self.lp_share.clone())],
        )
    }

    pub fn unbond(
        &mut self,
        sender: &str,
        amount: u128,
        unbonding_period: impl Into<Option<u64>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::Unbond {
                tokens: amount.into(),
                unbonding_period: self.unbonding_period_or_default(unbonding_period),
            },
            &[],
        )
    }

    pub fn claim(&mut self, sender: &str) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.stake_contract.clone(),
            &ExecuteMsg::Claim {},
            &[],
        )
    }

    // call to vesting contract
    pub fn transfer(
        &mut self,
        sender: &str,
        recipient: &str,
        amount: impl Into<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(sender),
            self.token_contract.clone(),
            &Cw20ExecuteMsg::Transfer {
                recipient: recipient.into(),
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn distribute_funds<'s>(
        &mut self,
        executor: &str,
        sender: impl Into<Option<&'s str>>,
        funds: Option<AssetValidated>,
    ) -> AnyResult<AppResponse> {
        let sender = sender.into();

        if let Some(funds) = funds {
            let transfer_msg = funds.into_msg(self.stake_contract.clone())?;
            self.app
                .execute(Addr::unchecked(sender.unwrap_or(executor)), transfer_msg)?;
        }

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::DistributeRewards {
                sender: sender.map(str::to_owned),
            },
            &[],
        )
    }

    pub fn execute_fund_distribution<'s>(
        &mut self,
        executor: &str,
        sender: impl Into<Option<&'s str>>,
        funds: AssetValidated,
    ) -> AnyResult<AppResponse> {
        let _sender = sender.into();

        let curr_block = self.app.block_info().time;

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::FundDistribution {
                funding_info: FundingInfo {
                    start_time: curr_block.seconds(),
                    distribution_duration: 100,
                    amount: funds.amount,
                },
            },
            &[Coin {
                denom: funds.info.to_string(),
                amount: funds.amount,
            }],
        )
    }

    pub fn execute_fund_distribution_curve(
        &mut self,
        executor: &str,
        denom: impl Into<String>,
        amount: u128,
        distribution_duration: u64,
    ) -> AnyResult<AppResponse> {
        let curr_block = self.app.block_info().time;

        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::FundDistribution {
                funding_info: FundingInfo {
                    start_time: curr_block.seconds(),
                    distribution_duration,
                    amount: Uint128::from(amount),
                },
            },
            &[Coin {
                denom: denom.into(),
                amount: Uint128::new(amount),
            }],
        )
    }

    // call to staking contract by sender
    pub fn execute_fund_distribution_with_cw20(
        &mut self,
        executor: &str,
        funds: AssetValidated,
    ) -> AnyResult<AppResponse> {
        let funds_amount = funds.amount.u128();
        let curr_block = self.app.block_info().time;

        self.execute_fund_distribution_with_cw20_curve(
            executor,
            funds,
            FundingInfo {
                start_time: curr_block.seconds(),
                distribution_duration: 100,
                amount: Uint128::from(funds_amount),
            },
        )
    }

    pub fn execute_fund_distribution_with_cw20_curve(
        &mut self,
        executor: &str,
        funds: AssetValidated,
        funding_info: FundingInfo,
    ) -> AnyResult<AppResponse> {
        let token = match funds.info {
            AssetInfoValidated::Token(contract_addr) => contract_addr,
            _ => bail!("Only tokens are supported for cw20 distribution"),
        };
        self.app.execute_contract(
            Addr::unchecked(executor),
            token,
            &Cw20ExecuteMsg::Send {
                contract: self.stake_contract.to_string(),
                amount: funds.amount,
                msg: to_json_binary(&ReceiveMsg::Fund { funding_info })?,
            },
            &[],
        )
    }

    pub fn withdraw_funds<'s>(
        &mut self,
        executor: &str,
        owner: impl Into<Option<&'s str>>,
        receiver: impl Into<Option<&'s str>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            Addr::unchecked(executor),
            self.stake_contract.clone(),
            &ExecuteMsg::WithdrawRewards {
                owner: owner.into().map(str::to_owned),
                receiver: receiver.into().map(str::to_owned),
            },
            &[],
        )
    }

    pub fn withdrawable_rewards(&self, owner: &str) -> StdResult<Vec<AssetValidated>> {
        let resp: WithdrawableRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::WithdrawableRewards {
                owner: owner.to_owned(),
            },
        )?;
        Ok(resp.rewards)
    }

    pub fn distributed_funds(&self) -> StdResult<Vec<AssetValidated>> {
        let resp: DistributedRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::DistributedRewards {},
        )?;
        Ok(resp.distributed)
    }

    pub fn withdrawable_funds(&self) -> StdResult<Vec<AssetValidated>> {
        let resp: DistributedRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::DistributedRewards {},
        )?;
        Ok(resp.withdrawable)
    }

    pub fn undistributed_funds(&self) -> StdResult<Vec<AssetValidated>> {
        let resp: UndistributedRewardsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::UndistributedRewards {},
        )?;
        Ok(resp.rewards)
    }

    /// returns address' balance of native token
    pub fn query_balance(&self, address: &str, denom: &str) -> StdResult<u128> {
        let resp = self.app.wrap().query_balance(address, denom)?;
        Ok(resp.amount.u128())
    }

    pub fn query_cw20_balance(&self, address: &str, cw20: impl Into<String>) -> StdResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            cw20,
            &Cw20QueryMsg::Balance {
                address: address.to_owned(),
            },
        )?;
        Ok(balance.balance.u128())
    }

    // returns address' balance on vesting contract
    pub fn query_balance_vesting_contract(&self, address: &str) -> StdResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            self.token_contract.clone(),
            &Cw20QueryMsg::Balance {
                address: address.to_owned(),
            },
        )?;
        Ok(balance.balance.u128())
    }

    // returns address' balance on vesting contract
    pub fn query_balance_staking_contract(&self) -> StdResult<u128> {
        let balance: BalanceResponse = self.app.wrap().query_wasm_smart(
            self.token_contract.clone(),
            &Cw20QueryMsg::Balance {
                address: self.stake_contract.to_string(),
            },
        )?;
        Ok(balance.balance.u128())
    }

    pub fn query_staked(
        &self,
        address: &str,
        unbonding_period: impl Into<Option<u64>>,
    ) -> StdResult<u128> {
        let staked: StakedResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::Staked {
                address: address.to_owned(),
                unbonding_period: self.unbonding_period_or_default(unbonding_period),
            },
        )?;
        Ok(staked.stake.u128())
    }

    pub fn query_staked_periods(&self) -> StdResult<Vec<BondingPeriodInfo>> {
        let info: BondingInfoResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::BondingInfo {})?;
        Ok(info.bonding)
    }

    pub fn query_all_staked(&self, address: &str) -> StdResult<AllStakedResponse> {
        let all_staked: AllStakedResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::AllStaked {
                address: address.to_owned(),
            },
        )?;
        Ok(all_staked)
    }

    pub fn query_total_staked(&self) -> StdResult<u128> {
        let total_staked: TotalStakedResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::TotalStaked {})?;
        Ok(total_staked.total_staked.u128())
    }

    pub fn query_claims(&self, address: &str) -> StdResult<Vec<Claim>> {
        let claims: ClaimsResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::Claims {
                address: address.to_owned(),
            },
        )?;
        Ok(claims.claims)
    }

    pub fn query_annualized_rewards(
        &self,
    ) -> StdResult<Vec<(UnbondingPeriod, Vec<AnnualizedReward>)>> {
        let apr: AnnualizedRewardsResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::AnnualizedRewards {})?;
        Ok(apr.rewards)
    }

    pub fn query_rewards_power(&self, address: &str) -> StdResult<Vec<(AssetInfoValidated, u128)>> {
        let rewards: RewardsPowerResponse = self.app.wrap().query_wasm_smart(
            self.stake_contract.clone(),
            &QueryMsg::RewardsPower {
                address: address.to_owned(),
            },
        )?;

        Ok(rewards
            .rewards
            .into_iter()
            .map(|(a, p)| (a, p.u128()))
            .filter(|(_, p)| *p > 0)
            .collect())
    }

    pub fn query_total_rewards_power(&self) -> StdResult<Vec<(AssetInfoValidated, u128)>> {
        let rewards: RewardsPowerResponse = self
            .app
            .wrap()
            .query_wasm_smart(self.stake_contract.clone(), &QueryMsg::TotalRewardsPower {})?;

        Ok(rewards
            .rewards
            .into_iter()
            .map(|(a, p)| (a, p.u128()))
            .filter(|(_, p)| *p > 0)
            .collect())
    }
}
