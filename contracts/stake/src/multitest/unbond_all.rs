use std::vec;

use cosmwasm_std::{Addr, Decimal};
use cw_multi_test::Executor;
use dex::asset::AssetInfo;

use crate::{
    msg::ExecuteMsg,
    multitest::suite::{juno, native_token, SuiteBuilder},
    ContractError,
};

use super::suite::SEVEN_DAYS;

const UNBONDER: &str = "unbonder";

#[test]
fn delegate_and_unbond() {
    let user = "user";
    let mut suite = SuiteBuilder::new()
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .build();

    // Delegate half of the tokens for 7 days (default with None).
    suite.delegate(user, 50_000u128, None).unwrap();

    // Unbond with unbond flag to true.
    suite.unbond(user, 50_000u128, None).unwrap();

    // Vesting contract has no token since sent back to user.
    assert_eq!(suite.query_balance_vesting_contract(user).unwrap(), 0u128);

    // Total stake is zero.
    assert_eq!(suite.query_total_staked().unwrap(), 0u128);

    let claims = suite.query_claims(user).unwrap();
    assert_eq!(claims.len(), 1);

    assert_eq!(suite.query_balance_vesting_contract(user).unwrap(), 0u128);
}

#[test]
fn single_delegate_unbond_and_claim() {
    let user = "user";
    let mut suite = SuiteBuilder::new()
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .build();

    // Delegate half of the tokens for 7 days (default with None).
    suite.delegate(user, 50_000u128, None).unwrap();

    // Unbond.
    suite.unbond(user, 25_000u128, None).unwrap();

    // Staking contract has all tokens previously deposited
    assert_eq!(suite.query_balance_staking_contract().unwrap(), 50_000u128);

    // Staking tokens are half of the delegated
    assert_eq!(suite.query_total_staked().unwrap(), 25_000u128);

    // Claim is there since made before unbond all.
    let claims = suite.query_claims(user).unwrap();
    assert_eq!(claims.len(), 1);

    // Free locked tokens.
    suite.update_time(SEVEN_DAYS * 2);
    suite.claim(user).unwrap();
    // User has not delegated tokens + delegated and then unbonded.
    assert_eq!(suite.query_total_staked().unwrap(), 25_000u128);
}

#[test]
fn multiple_delegate_unbond_and_claim_with_unbond_all() {
    let user = "user";
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![SEVEN_DAYS, SEVEN_DAYS * 3])
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .with_unbonder(UNBONDER)
        .build();

    // Delegate half of the tokens for 7 days (default with None).
    suite.delegate(user, 50_000u128, SEVEN_DAYS).unwrap();

    // Delegate half of the tokens for 21 days.
    suite.delegate(user, 50_000u128, SEVEN_DAYS * 3).unwrap();

    // Unbond.
    suite.unbond(user, 25_000u128, None).unwrap();

    // Staking contract has all initial tokens.
    assert_eq!(suite.query_balance_staking_contract().unwrap(), 100_000u128);

    // Tokens in stake are 100_000 minus unbonded.
    assert_eq!(suite.query_total_staked().unwrap(), 75_000u128);

    // Claim is there since made before unbond all.
    let claims = suite.query_claims(user).unwrap();
    assert_eq!(claims.len(), 1);

    suite.update_time(SEVEN_DAYS * 2);
    suite.claim(user).unwrap();

    assert_eq!(suite.query_staked(user, SEVEN_DAYS).unwrap(), 25_000u128);

    // Unbond tokens delegated for 21 days.
    suite.unbond(user, 25_000u128, SEVEN_DAYS * 3).unwrap();

    let claims = suite.query_claims(user).unwrap();
    assert_eq!(claims.len(), 1);

    // User has previously claimed tokens + unbonded tokens from 21 days.
    assert_eq!(
        suite
            .query_all_staked(user)
            .unwrap()
            .stakes
            .into_iter()
            .map(|stake_el| u128::from(stake_el.stake))
            .sum::<u128>(),
        50_000u128
    );
    // last unbonded by user is 100k - 25k
    assert_eq!(suite.query_balance_staking_contract().unwrap(), 75_000u128);
}

#[test]
fn delegate_with_unbond_all_flag() {
    let user = "user";
    let admin = "admin";
    let mut suite = SuiteBuilder::new()
        .with_admin(admin)
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .with_unbonder(UNBONDER)
        .build();

    // Set unbond all flag to true.
    let stake_contract = suite.stake_contract();
    suite
        .app
        .execute_contract(
            Addr::unchecked(UNBONDER),
            Addr::unchecked(stake_contract),
            &ExecuteMsg::UnbondAll {},
            &[],
        )
        .unwrap();

    // Cannot delegate if unbond all.
    let err = suite.delegate(user, 50_000u128, None).unwrap_err();
    assert_eq!(
        ContractError::CannotDelegateIfUnbondAll {},
        err.downcast().unwrap()
    );
}

#[test]
fn delegate_as_with_unbond_all_flag() {
    let user = "factory";
    let user2 = "client";
    let mut suite = SuiteBuilder::new()
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .with_unbonder(UNBONDER)
        .build();

    // Set unbond all flag to true.
    let stake_contract = suite.stake_contract();
    suite
        .app
        .execute_contract(
            Addr::unchecked(UNBONDER),
            Addr::unchecked(stake_contract),
            &ExecuteMsg::UnbondAll {},
            &[],
        )
        .unwrap();

    // Cannot delegate through cw20 contract if unbond all.
    let err = suite
        .delegate_as(user, 50_000u128, None, Some(user2))
        .unwrap_err();

    assert_eq!(
        ContractError::CannotDelegateIfUnbondAll {},
        err.downcast().unwrap()
    );
}

#[test]
fn multiple_distribution_flows() {
    let user = "user";
    let unbonding_period = 1000u64;

    let mut suite = SuiteBuilder::new()
        .with_admin("admin")
        .with_unbonder(UNBONDER)
        .with_unbonding_periods(vec![unbonding_period])
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .with_native_balances("juno", vec![(user, 1200)])
        .with_native_balances("wynd", vec![(user, 1200)])
        .build();

    // Distribution flow
    suite
        .create_distribution_flow(
            "admin",
            user,
            AssetInfo::SmartToken("juno".to_string()),
            vec![(unbonding_period, Decimal::one())],
        )
        .unwrap();

    suite
        .create_distribution_flow(
            "admin",
            user,
            AssetInfo::SmartToken("wynd".to_string()),
            vec![(unbonding_period, Decimal::one())],
        )
        .unwrap();

    suite.delegate(user, 1_000, unbonding_period).unwrap();

    // Fund both distribution flows with same amount.
    suite
        .execute_fund_distribution(user, None, juno(400))
        .unwrap();
    suite
        .execute_fund_distribution(user, None, native_token("wynd".to_string(), 400u128))
        .unwrap();

    suite.update_time(100);

    // Set unbond all flag to true.
    let stake_contract = suite.stake_contract();
    suite
        .app
        .execute_contract(
            Addr::unchecked(UNBONDER),
            Addr::unchecked(stake_contract),
            &ExecuteMsg::UnbondAll {},
            &[],
        )
        .unwrap();
    // Cannot distribute funds when unbod all.
    let err = suite.distribute_funds(user, None, None).unwrap_err();

    assert_eq!(
        ContractError::CannotDistributeIfUnbondAll {
            what: "rewards".into()
        },
        err.downcast().unwrap()
    );
}
