use cosmwasm_std::{assert_approx_eq, Decimal};
use dex::asset::{AssetInfo, AssetInfoExt, AssetInfoValidated};

use crate::multitest::suite::SuiteBuilder;

use super::suite::Suite;

const DAY: u64 = 24 * 60 * 60;
const UNBONDING_PERIODS: &[u64; 2] = &[DAY, 2 * DAY];
const ADMIN: &str = "owner";
const UNBONDER: &str = "unbonder";
const REWARDS_DISTRIBUTOR: &str = "rewardsdistributor";
const VOTER1: &str = "voter1";
const VOTER2: &str = "voter2";
const VOTER3: &str = "voter3";

fn cash() -> AssetInfoValidated {
    AssetInfoValidated::SmartToken("cash".to_string())
}

fn initial_setup() -> Suite {
    let mut suite = SuiteBuilder::new()
        .with_admin(ADMIN)
        .with_unbonder(UNBONDER)
        .with_min_bond(0)
        .with_tokens_per_power(1)
        .with_unbonding_periods(UNBONDING_PERIODS.to_vec())
        .with_native_balances("cash", vec![(REWARDS_DISTRIBUTOR, 100_000)])
        .with_lp_share_denom("tia".to_string())
        .with_native_balances(
            "tia",
            vec![
                (VOTER1, 500),
                (VOTER2, 600),
                (VOTER3, 450),
                (UNBONDER, 1000),
            ],
        )
        .build();

    suite
        .create_distribution_flow(
            ADMIN,
            REWARDS_DISTRIBUTOR,
            cash().into(),
            vec![
                (UNBONDING_PERIODS[0], Decimal::percent(100)),
                (UNBONDING_PERIODS[1], Decimal::percent(200)),
            ],
        )
        .unwrap();

    suite.delegate(VOTER1, 500, UNBONDING_PERIODS[0]).unwrap();
    suite.delegate(VOTER2, 600, UNBONDING_PERIODS[1]).unwrap();
    suite.delegate(VOTER3, 450, UNBONDING_PERIODS[0]).unwrap();

    suite.unbond(VOTER2, 100, UNBONDING_PERIODS[1]).unwrap();
    suite.unbond(VOTER3, 450, UNBONDING_PERIODS[0]).unwrap();

    // at this point, we have:
    assert_eq!(
        suite.query_rewards_power(VOTER1).unwrap()[0].1,
        500,
        "500 in period 1 => power 500"
    );
    assert_eq!(
        suite.query_rewards_power(VOTER2).unwrap()[0].1,
        1000,
        "600 in period 1, 600 in period 2 => power 1200 - unbonded"
    );
    assert!(
        suite.query_rewards_power(VOTER3).unwrap().is_empty(),
        "no stake in any period"
    );
    assert_eq!(
        suite.query_total_rewards_power().unwrap()[0].1,
        1500,
        "500 + 1000 = 1500"
    );

    suite
        .distribute_funds(
            REWARDS_DISTRIBUTOR,
            REWARDS_DISTRIBUTOR,
            Some(cash().with_balance(1500u128)),
        )
        .unwrap();

    // validate rewards:
    assert_eq!(
        suite.withdrawable_rewards(VOTER1).unwrap()[0].amount.u128(),
        500,
        "500 / 1500 * 1500 = 500"
    );
    assert_eq!(
        suite.withdrawable_rewards(VOTER2).unwrap()[0].amount.u128(),
        1000,
        "1000 / 1500 * 1500 = 1000"
    );
    assert_eq!(
        suite.withdrawable_rewards(VOTER3).unwrap()[0].amount.u128(),
        0,
        "0 / 1500 * 1500 = 0"
    );

    suite
}

fn run_checks(suite: Suite) {
    // at this point, we have:
    assert_eq!(
        suite.query_rewards_power(VOTER1).unwrap()[0].1,
        500,
        "500 in period 1 => power 500"
    );
    assert!(
        suite.query_rewards_power(VOTER2).unwrap().is_empty(),
        "no stake in any period"
    );
    assert!(
        suite.query_rewards_power(VOTER3).unwrap().is_empty(),
        "no stake in any period"
    );
    assert_eq!(suite.query_total_rewards_power().unwrap()[0].1, 500);

    // check unstaked LP balance
    assert_eq!(suite.query_balance(VOTER1, "tia").unwrap(), 0);
    assert_eq!(suite.query_balance(VOTER2, "tia").unwrap(), 300);
    assert_eq!(suite.query_balance(VOTER3, "tia").unwrap(), 450);

    // check withdrawable rewards
    assert_approx_eq!(
        suite.withdrawable_rewards(VOTER1).unwrap()[0].amount.u128(),
        1300,
        "0.001",
        "500 + 800 = 1300",
    );
    assert_eq!(
        suite.withdrawable_rewards(VOTER2).unwrap()[0].amount.u128(),
        1000,
        "same as before"
    );
    assert_eq!(
        suite.withdrawable_rewards(VOTER3).unwrap()[0].amount.u128(),
        0,
        "same as before"
    );

    assert_eq!(
        suite.query_staked(VOTER1, UNBONDING_PERIODS[0]).unwrap(),
        500
    );
    assert_eq!(suite.query_staked(VOTER1, UNBONDING_PERIODS[1]).unwrap(), 0);
    assert_eq!(suite.query_staked(VOTER2, UNBONDING_PERIODS[0]).unwrap(), 0);
    assert_eq!(suite.query_staked(VOTER2, UNBONDING_PERIODS[1]).unwrap(), 0);
    assert_eq!(suite.query_staked(VOTER3, UNBONDING_PERIODS[0]).unwrap(), 0);
    assert_eq!(suite.query_staked(VOTER3, UNBONDING_PERIODS[1]).unwrap(), 0);
    assert_eq!(suite.query_total_staked().unwrap(), 500);

    let bonding_infos = suite.query_staked_periods().unwrap();
    assert_eq!(bonding_infos[0].total_staked.u128(), 500);
    assert_eq!(bonding_infos[1].total_staked.u128(), 0);

    // check claims
    assert_eq!(suite.query_claims(VOTER1).unwrap().len(), 0);
    assert_eq!(suite.query_claims(VOTER2).unwrap().len(), 1);
    assert_eq!(suite.query_claims(VOTER3).unwrap().len(), 0);
}

#[test]
fn control_case() {
    let mut suite = initial_setup();

    suite.unbond(VOTER2, 200, UNBONDING_PERIODS[1]).unwrap();

    suite.update_time(DAY);

    suite.unbond(VOTER2, 300, UNBONDING_PERIODS[1]).unwrap();

    suite.update_time(DAY);

    suite.claim(VOTER2).unwrap();
    suite.claim(VOTER3).unwrap();

    suite
        .distribute_funds(
            REWARDS_DISTRIBUTOR,
            REWARDS_DISTRIBUTOR,
            Some(cash().with_balance(800u128)),
        )
        .unwrap();

    run_checks(suite);
}

#[test]
fn multiple_distributions() {
    let mut suite = SuiteBuilder::new()
        .with_admin(ADMIN)
        .with_min_bond(100) // also make power calculation a bit more interesting
        .with_tokens_per_power(10)
        .with_unbonding_periods(UNBONDING_PERIODS.to_vec())
        .with_lp_share_denom("tia".to_string())
        .with_native_balances("cash", vec![(REWARDS_DISTRIBUTOR, 100_000)])
        .with_native_balances("juno", vec![(REWARDS_DISTRIBUTOR, 100_000)])
        .with_native_balances("tia", vec![(VOTER1, 10), (VOTER2, 100), (VOTER3, 200)])
        .build();

    suite
        .create_distribution_flow(
            ADMIN,
            REWARDS_DISTRIBUTOR,
            cash().into(),
            vec![
                (UNBONDING_PERIODS[0], Decimal::percent(100)),
                (UNBONDING_PERIODS[1], Decimal::percent(200)),
            ],
        )
        .unwrap();

    suite
        .create_distribution_flow(
            ADMIN,
            REWARDS_DISTRIBUTOR,
            AssetInfo::SmartToken("juno".to_string()),
            vec![
                (UNBONDING_PERIODS[0], Decimal::percent(100)),
                (UNBONDING_PERIODS[1], Decimal::percent(200)),
            ],
        )
        .unwrap();

    suite.delegate(VOTER1, 10, UNBONDING_PERIODS[1]).unwrap();
    suite.delegate(VOTER2, 100, UNBONDING_PERIODS[1]).unwrap();
    suite.delegate(VOTER3, 200, UNBONDING_PERIODS[1]).unwrap();

    // at this point, we have:
    assert!(
        suite.query_rewards_power(VOTER1).unwrap().is_empty(),
        "10 in period 2 < MIN_BOND"
    );
    assert_eq!(
        suite.query_rewards_power(VOTER2).unwrap()[0].1,
        20,
        "100 in period 1 that has %200 percentage => power 200 / 10 = 20"
    );
    assert_eq!(
        suite.query_rewards_power(VOTER3).unwrap()[0].1,
        40,
        "200 in period 1 that has %200 percentage => power 400 / 10 = 40"
    );
    // => total power is 60

    // distribute 3000 cash and 1500 juno
    suite
        .distribute_funds(
            REWARDS_DISTRIBUTOR,
            REWARDS_DISTRIBUTOR,
            Some(cash().with_balance(3000u128)),
        )
        .unwrap();
    suite
        .distribute_funds(
            REWARDS_DISTRIBUTOR,
            REWARDS_DISTRIBUTOR,
            Some(AssetInfoValidated::SmartToken("juno".to_string()).with_balance(1500u128)),
        )
        .unwrap();

    fn assert_rewards(suite: &mut Suite) {
        // summing balance and withdrawable rewards, because some have withdrawn
        let voter1_cash = suite.query_balance(VOTER1, "cash").unwrap();
        let voter2_cash = suite.query_balance(VOTER2, "cash").unwrap();
        let voter3_cash = suite.query_balance(VOTER3, "cash").unwrap();
        let voter1_juno = suite.query_balance(VOTER1, "juno").unwrap();
        let voter2_juno = suite.query_balance(VOTER2, "juno").unwrap();
        let voter3_juno = suite.query_balance(VOTER3, "juno").unwrap();

        let voter1_rewards = suite.withdrawable_rewards(VOTER1).unwrap();
        let voter2_rewards = suite.withdrawable_rewards(VOTER2).unwrap();
        let voter3_rewards = suite.withdrawable_rewards(VOTER3).unwrap();

        // assert cash rewards
        assert_eq!(
            voter1_rewards[0].amount.u128() + voter1_cash,
            0,
            "no power => no rewards"
        );
        assert_eq!(
            voter2_rewards[0].amount.u128() + voter2_cash,
            1000,
            "20 / 60 * 3000 = 1000"
        );
        assert_eq!(
            voter3_rewards[0].amount.u128() + voter3_cash,
            2000,
            "40 / 60 * 3000 = 2000"
        );
        // assert juno rewards
        assert_eq!(
            voter1_rewards[1].amount.u128() + voter1_juno,
            0,
            "no power => no rewards"
        );
        assert_eq!(
            voter2_rewards[1].amount.u128() + voter2_juno,
            500,
            "20 / 60 * 1500 = 500"
        );
        assert_eq!(
            voter3_rewards[1].amount.u128() + voter3_juno,
            1000,
            "40 / 60 * 1500 = 1000"
        );
    }

    assert_rewards(&mut suite);

    // withdraw some rewards before unbonding
    suite.withdraw_funds(VOTER2, None, None).unwrap();
    // unbond all
    suite.unbond(VOTER1, 10, UNBONDING_PERIODS[1]).unwrap();
    suite.unbond(VOTER2, 100, UNBONDING_PERIODS[1]).unwrap();
    suite.unbond(VOTER3, 200, UNBONDING_PERIODS[1]).unwrap();

    // rewards should stay the same
    assert_rewards(&mut suite);

    // unbond doesn't return the initially delegated tokens
    // assert token balances
    // assert_eq!(suite.query_balance(VOTER1, "tia").unwrap(), 10);
    // assert_eq!(suite.query_balance(VOTER2, "tia").unwrap(), 100);
    // assert_eq!(suite.query_balance(VOTER3, "tia").unwrap(), 200);

    // no claims created and none left
    // assert!(suite.query_claims(VOTER1).unwrap().is_empty());
    // assert!(suite.query_claims(VOTER2).unwrap().is_empty());
    // assert!(suite.query_claims(VOTER3).unwrap().is_empty());
}
