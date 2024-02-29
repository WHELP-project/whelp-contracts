use std::{any::Any, vec};

use cosmwasm_std::{to_json_binary, Addr, Empty, StdError, Uint128};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};
use dex::stake::ReceiveMsg;

use crate::{
    contract::{execute, instantiate, query},
    msg::{ExecuteMsg, MigrateMsg, QueryMsg, UnbondAllResponse},
    multitest::suite::{contract_stake, SuiteBuilder},
};

// const UNBONDER: &str = "unbonder";
const MINTER: &str = "minter";
const USER: &str = "user";
const UNBONDER: &str = "unbonder";
const ADMIN: &str = "admin";
pub const SEVEN_DAYS: u64 = 604800;

#[test]
fn stake_old_migrate_with_unbond_all_and_unbond() {
    let mut app = App::default();

    let admin = Addr::unchecked(ADMIN);

    let suite = SuiteBuilder::new()
        .with_native_balances("VEST", vec![(USER, 1_000_000)])
        .with_lp_share_denom("VEST".to_string())
        .with_admin(ADMIN)
        .with_unbonder(UNBONDER)
        .with_unbonding_periods(vec![SEVEN_DAYS])
        .build();

    // Check that UnbondAll is not present.
    // let err: Result<UnbondAllResponse, StdError> = app
    //     .wrap()
    //     .query_wasm_smart(old_contract..clone(), &QueryMsg::UnbondAll {});

    assert!(matches!(err.unwrap_err(), StdError::GenericErr { .. }));

    // Delegate tokens into old contract.
    app.execute_contract(
        Addr::unchecked(USER),
        token_contract.clone(),
        &Cw20ExecuteMsg::Send {
            contract: stake_old_contract.to_string(),
            amount: 500_000u128.into(),
            msg: to_json_binary(&ReceiveMsg::Delegate {
                unbonding_period: SEVEN_DAYS,
                delegate_as: None,
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // Check tokens are correctly delegated.
    let total_staked_resp: TotalStakedResponse = app
        .wrap()
        .query_wasm_smart(stake_old_contract.clone(), &QueryMsg::TotalStaked {})
        .unwrap();

    assert_eq!(Uint128::new(500_000), total_staked_resp.total_staked,);

    // Upload new bytecode.
    let new_contract: Box<dyn Contract<Empty>> = Box::new(
        ContractWrapper::new_with_empty(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        )
        .with_migrate(crate::contract::migrate),
    );
    let stake_new_id = app.store_code(new_contract);

    // Migrate to new contract with unbond all.
    app.migrate_contract(
        admin,
        stake_old_contract.clone(),
        &MigrateMsg {
            unbonder: Some(UNBONDER.to_owned()),
            unbond_all: true,
        },
        stake_new_id,
    )
    .unwrap();

    // Check that unbond all has been correctly set.
    let resp: UnbondAllResponse = app
        .wrap()
        .query_wasm_smart(stake_old_contract.clone(), &QueryMsg::UnbondAll {})
        .unwrap();

    assert!(resp.unbond_all);

    let balance: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            token_contract.clone(),
            &Cw20QueryMsg::Balance {
                address: USER.to_owned(),
            },
        )
        .unwrap();

    // Assert that user has initial tokens - staked tokens.
    assert_eq!(Uint128::new(500_000), balance.balance,);

    // Unbond tokens staked in old contract
    app.execute_contract(
        Addr::unchecked(USER),
        stake_old_contract,
        &ExecuteMsg::Unbond {
            tokens: Uint128::new(500_000),
            unbonding_period: SEVEN_DAYS,
        },
        &[],
    )
    .unwrap();

    let balance: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            token_contract,
            &Cw20QueryMsg::Balance {
                address: USER.to_owned(),
            },
        )
        .unwrap();

    // Assert that user has initial tokens.
    assert_eq!(Uint128::new(1_000_000), balance.balance,);
}
