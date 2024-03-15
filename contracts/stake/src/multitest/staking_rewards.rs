use cosmwasm_std::{Decimal, OverflowError, OverflowOperation, StdError, Uint128};
use dex::asset::{AssetInfo, AssetInfoValidated};
use dex::stake::UnbondingPeriod;

use crate::error::ContractError;
use crate::msg::{AllStakedResponse, StakedResponse};
use crate::multitest::suite::juno_power;

use super::suite::SuiteBuilder;
use test_case::test_case;

#[test]
fn unbond_overflow() {
    let unbonding_period = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![(unbonding_period)])
        .build();

    let err = suite.unbond("user", 1u128, unbonding_period).unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            0,
            1
        ))),
        err.downcast().unwrap()
    );
}

#[test]
fn no_unbonding_period_found() {
    let user1 = "user1";
    let unbonding_period = 1000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![unbonding_period])
        .with_lp_share_denom("TIA".to_string())
        .with_native_balances("TIA", vec![(user1, 100_000)])
        .build();

    let err = suite
        .delegate(user1, 12_000u128, unbonding_period + 1)
        .unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(unbonding_period + 1),
        err.downcast().unwrap()
    );

    suite.delegate(user1, 12_000u128, unbonding_period).unwrap();

    let err = suite
        .unbond(user1, 12_000u128, unbonding_period + 1)
        .unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(unbonding_period + 1),
        err.downcast().unwrap()
    );

    suite.unbond(user1, 12_000u128, unbonding_period).unwrap();
}

#[test]
fn one_user_multiple_unbonding_periods() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![
            unbonding_period1,
            unbonding_period2,
            unbonding_period3,
        ])
        .with_lp_share_denom("TIA".to_string())
        .with_native_balances("TIA", vec![(user, 100_000)])
        .build();

    let bonds = [20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // unbond on second unbonding period
    suite.unbond(user, 20_000u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        10_000u128
    );

    // top some more on first unbonding period
    suite.delegate(user, 5_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        25_000u128
    );

    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(25_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    lp_share_denom: "TIA".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    lp_share_denom: "TIA".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    lp_share_denom: "TIA".to_string(),
                },
            ]
        }
    );

    let periods = suite.query_staked_periods().unwrap();
    assert_eq!(periods.len(), 3);
    assert_eq!(periods[0].unbonding_period, unbonding_period1);
    assert_eq!(periods[0].total_staked.u128(), 25_000);
    assert_eq!(periods[1].unbonding_period, unbonding_period2);
    assert_eq!(periods[1].total_staked.u128(), 10_000);
    assert_eq!(periods[2].unbonding_period, unbonding_period3);
    assert_eq!(periods[2].total_staked.u128(), 10_000);
}

#[test]
fn multiple_users_multiple_unbonding_periods() {
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;

    let bonds = [20_000u128, 30_000u128, 10_000u128, 16_000u128, 6_000u128];
    let delegated: u128 = bonds.iter().sum();
    let members = ["user1", "user2", "user3"];

    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![
            unbonding_period1,
            unbonding_period2,
            unbonding_period3,
        ])
        .with_admin("admin")
        .with_min_bond(4_500)
        .with_lp_share_denom("tia".to_string())
        .with_native_balances(
            "tia",
            vec![(user1, 100_000), (user2, 100_000), (user3, 100_000)],
        )
        .build();

    suite
        .create_distribution_flow(
            "admin",
            members[0],
            AssetInfo::SmartToken("juno".to_string()),
            vec![
                (unbonding_period1, Decimal::percent(1)),
                (unbonding_period2, Decimal::percent(40)),
                (unbonding_period3, Decimal::percent(60)),
            ],
        )
        .unwrap();

    suite
        .delegate(members[0], bonds[0], unbonding_period1)
        .unwrap();
    suite
        .delegate(members[1], bonds[1], unbonding_period2)
        .unwrap();
    suite
        .delegate(members[0], bonds[2], unbonding_period3)
        .unwrap();
    suite
        .delegate(members[2], bonds[3], unbonding_period2)
        .unwrap();
    suite
        .delegate(members[2], bonds[4], unbonding_period3)
        .unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // first user unbonds on second unbonding period
    suite.unbond(user1, 20_000u128, unbonding_period1).unwrap();
    assert_eq!(suite.query_staked(user1, unbonding_period1).unwrap(), 0u128);
    assert_eq!(
        suite.query_staked(user1, unbonding_period3).unwrap(),
        10_000u128
    );

    assert_eq!(
        suite.query_rewards_power(user1).unwrap(),
        vec![(AssetInfoValidated::SmartToken("juno".to_string()), 6u128)]
    ); // same as before

    assert_eq!(suite.query_total_rewards_power().unwrap(), juno_power(27)); // same as before
}

#[test_case(vec![1000,4000, 8000],vec![20000,30000,20000] => Some(38); "should success")]
fn query_all_staked(stake_config: Vec<UnbondingPeriod>, amount: Vec<u128>) -> Option<u64> {
    let user = "user";

    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(stake_config.clone())
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .build();

    for i in 0..=(stake_config.len() - 1) {
        // delegate unbonding period
        suite.delegate(user, amount[i], stake_config[i]).unwrap();
        // This works
        suite.query_staked(user, stake_config[i]).unwrap();
        // This works
        suite.query_all_staked(user).unwrap();

        assert_eq!(
            suite.query_staked(user, stake_config[i]).unwrap(),
            amount[i]
        );
    }

    // This works
    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(20_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    lp_share_denom: "tia".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(30_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    lp_share_denom: "tia".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(20_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    lp_share_denom: "tia".to_string(),
                },
            ]
        }
    );
    Some(38u64)
}

#[test]
fn delegate_unbond_under_min_bond() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![unbonding_period1, unbonding_period2])
        .with_min_bond(2_000)
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .build();

    // delegating first amount works (5_000 * 0.4 = 2_000)
    suite.delegate(user, 5_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        5_000u128
    );

    // delegating another amount under min bond doesn't increase voting power
    // 1_800 < 2_000
    suite.delegate(user, 1_800u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        1_800u128
    );

    // once the stake hits min_bond (2_000), count it, even if voting power (2_000 * 0.8 = 1_600) is still under min_bond
    suite.delegate(user, 200u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        2_000u128
    );

    suite.delegate(user, 5_000u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        7_000u128
    );

    // undelegate tokens from first pool so that delegation goes under min_bond
    suite.unbond(user, 3_500u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        1_500u128
    );
}

#[test]
fn one_user_multiple_periods_unbond_then_bond() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![
            unbonding_period1,
            unbonding_period2,
            unbonding_period3,
        ])
        .with_admin("admin")
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 125_000)])
        .build();

    suite
        .create_distribution_flow(
            "admin",
            user,
            AssetInfo::SmartToken("juno".to_string()),
            vec![
                (unbonding_period1, Decimal::percent(25)),
                (unbonding_period2, Decimal::percent(60)),
                (unbonding_period3, Decimal::percent(80)),
            ],
        )
        .unwrap();

    let bonds = [20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // unbond then delegate again
    suite.unbond(user, 20_000u128, unbonding_period1).unwrap();
    suite.unbond(user, 20_000u128, unbonding_period2).unwrap();
    assert_eq!(suite.query_staked(user, unbonding_period1).unwrap(), 0u128);
    suite.delegate(user, 20_000u128, unbonding_period1).unwrap();
    suite.delegate(user, 20_000u128, unbonding_period2).unwrap();

    assert_eq!(suite.query_total_staked().unwrap(), 60_000u128);

    assert_eq!(
        suite.query_rewards_power(user).unwrap(),
        vec![(AssetInfoValidated::SmartToken("juno".to_string()), 31u128)]
    );

    // 0.25 * 20_000 + 0.6 * 30_000 + 0.8 * 10_000
    assert_eq!(suite.query_total_rewards_power().unwrap(), juno_power(31));

    // top some more on first unbonding period but not more than we originally topped up
    suite.delegate(user, 25_000u128, unbonding_period1).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        45_000u128
    );
    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(45_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    lp_share_denom: "tia".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(30_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    lp_share_denom: "tia".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(10_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    lp_share_denom: "tia".to_string(),
                },
            ]
        }
    );
    assert_eq!(
        suite.query_rewards_power(user).unwrap(),
        vec![(AssetInfoValidated::SmartToken("juno".to_string()), 37u128)]
    );

    // 0.25 * 45_000 + 0.6 * 30_000 + 0.8 * 10_000
    assert_eq!(suite.query_total_rewards_power().unwrap(), juno_power(37));
}

#[test]
fn unbond_then_unbond_again() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![
            unbonding_period1,
            unbonding_period2,
            unbonding_period3,
        ])
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .build();

    // delegate on first unbonding period
    suite
        .delegate(user, 100_000u128, unbonding_period1)
        .unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        100_000u128
    );

    // manual rebond 40% of tokens to bucket 2
    suite.unbond(user, 40_000u128, unbonding_period1).unwrap();

    suite.update_time(unbonding_period1 + 1);
    suite.claim(user).unwrap();
    suite.delegate(user, 40_000u128, unbonding_period2).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period1).unwrap(),
        60_000u128
    );

    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        40_000u128
    );

    // manual rebond half of bucket 2 tokens to bucket 3
    suite.unbond(user, 20_000u128, unbonding_period2).unwrap();
    suite.update_time(unbonding_period2 + 1);
    suite.claim(user).unwrap();
    suite.delegate(user, 20_000u128, unbonding_period3).unwrap();
    assert_eq!(
        suite.query_staked(user, unbonding_period2).unwrap(),
        20_000u128
    );

    assert_eq!(
        suite.query_staked(user, unbonding_period3).unwrap(),
        20_000u128
    );

    assert_eq!(
        suite.query_all_staked(user).unwrap(),
        AllStakedResponse {
            stakes: vec![
                StakedResponse {
                    stake: Uint128::new(60_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 1000,
                    lp_share_denom: "tia".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(20_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 4000,
                    lp_share_denom: "tia".to_string(),
                },
                StakedResponse {
                    stake: Uint128::new(20_000),
                    total_locked: Uint128::zero(),
                    unbonding_period: 8000,
                    lp_share_denom: "tia".to_string(),
                },
            ]
        }
    );
}

#[test]
fn one_user_multiple_periods_delegate_or_unbond_fail() {
    let user = "user";
    let unbonding_period1 = 1000u64;
    let unbonding_period2 = 4000u64;
    let unbonding_period3 = 8000u64;
    let mut suite = SuiteBuilder::new()
        .with_unbonding_periods(vec![
            unbonding_period1,
            unbonding_period2,
            unbonding_period3,
        ])
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("tia", vec![(user, 100_000)])
        .build();

    let bonds = [20_000u128, 30_000u128, 10_000u128];
    let delegated: u128 = bonds.iter().sum();

    suite.delegate(user, bonds[0], unbonding_period1).unwrap();
    suite.delegate(user, bonds[1], unbonding_period2).unwrap();
    suite.delegate(user, bonds[2], unbonding_period3).unwrap();

    assert_eq!(suite.query_balance_staking_contract().unwrap(), delegated);

    // Fail case, unbonding 50_000 from a bucket with 20_000
    let err = suite
        .unbond(user, 50_000u128, unbonding_period1)
        .unwrap_err();
    assert_eq!(
        ContractError::Std(StdError::overflow(OverflowError::new(
            OverflowOperation::Sub,
            20000u128,
            50000u128
        ))),
        err.downcast().unwrap()
    );

    // Fail case, bonding to a non-existent bucket
    let err = suite.delegate(user, 10_000u128, 12000).unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(12000),
        err.downcast().unwrap()
    );

    // Fail case, unbonding from a non-existent bucket
    let err = suite.unbond(user, 50_000u128, 2000).unwrap_err();
    assert_eq!(
        ContractError::NoUnbondingPeriodFound(2000),
        err.downcast().unwrap()
    );
}
