use super::suite::{SuiteBuilder, SEVEN_DAYS};
use cosmwasm_std::Uint128;
use cw_controllers::Claim;

const AMOUNT: u128 = 100_000;
const DENOM: &str = "VEST";
const USER: &str = "user_addr_0000";

#[test]
fn delegate_and_unbond_tokens_still_vested() {
    let balances = vec![(USER, AMOUNT)];
    let mut suite = SuiteBuilder::new()
        .with_native_balances(DENOM, balances)
        .with_lp_share_denom(DENOM.to_string())
        .build();

    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        100_000u128
    );

    // delegate half of the tokens, ensure they are staked
    suite.delegate(USER, 50_000u128, None).unwrap();
    assert_eq!(suite.query_staked(USER, None).unwrap(), 50_000u128);
    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        50_000u128
    );
    assert_eq!(
        suite
            .query_balance_vesting_contract(&suite.stake_contract())
            .unwrap(),
        50_000u128
    );

    // undelegate and unbond all
    suite.unbond(USER, 50_000u128, None).unwrap();
    // nothing is staked
    assert_eq!(suite.query_staked(USER, None).unwrap(), 0u128);
    // Balance is the same until claim is available
    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        50_000u128
    );

    let claims = suite.query_claims(USER).unwrap();
    assert_eq!(claims.len(), 1);
    assert!(matches!(
        claims[0],
        Claim {
            amount,
            ..
        } if amount == Uint128::new(50_000)
    ));

    suite.update_time(SEVEN_DAYS * 2); // update height to simulate passing time
    suite.claim(USER).unwrap();
    let claims = suite.query_claims(USER).unwrap();
    assert_eq!(claims.len(), 0);
    // after expiration time passed, tokens can be claimed and transferred back to user account
    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        100_000u128
    );
}

#[test]
fn mixed_vested_liquid_delegate_and_transfer_remaining() {
    let balances = vec![(USER, AMOUNT)];
    let mut suite = SuiteBuilder::new()
        .with_native_balances(DENOM, balances)
        .with_lp_share_denom(DENOM.to_string())
        .build();

    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        100_000u128
    );

    suite.delegate(USER, 60_000u128, None).unwrap(); // delegate some of vested tokens as well
    assert_eq!(suite.query_staked(USER, None).unwrap(), 60_000u128);
    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        40_000u128
    );

    // transfer remaining 40_000 to a different address, to show that vested tokens are delegated
    // first
    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        40_000u128
    );
    suite
        .transfer(USER, "random_user", (40_000u128, DENOM.to_string()))
        .unwrap();

    assert_eq!(suite.query_balance_vesting_contract(USER).unwrap(), 0u128); // user has empty
                                                                            // account now

    // undelegate some of the tokens
    suite.unbond(USER, 20_000u128, None).unwrap();
    assert_eq!(suite.query_staked(USER, None).unwrap(), 40_000u128); // 60_000 delegated - 20_000
                                                                     // unbonded
    assert_eq!(suite.query_balance_vesting_contract(USER).unwrap(), 0u128);

    let claims = suite.query_claims(USER).unwrap();
    assert!(matches!(
        claims[0],
        Claim {
            amount,
            ..
        } if amount == Uint128::new(20_000)
    ));

    suite.update_time(SEVEN_DAYS); // update height to simulate passing time
    suite.claim(USER).unwrap();
    assert_eq!(
        suite.query_balance_vesting_contract(USER).unwrap(),
        20_000u128
    );
}
