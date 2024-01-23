use bindings_test::{mock_coreum_deps, CoreumApp};
use cosmwasm_std::{testing::mock_env, Addr, Decimal};
use cw_multi_test::{ContractWrapper, Executor};

use crate::{
    contract::execute,
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::Config,
};

#[test]
fn init_works() {
    let mut app = CoreumApp::default();

    let code_id = store_fee_splitter_code(&mut app);
    let sender = "addr0000";

    let first_tupple = ("tokenA".to_string(), Decimal::from_ratio(1u128, 2u128));
    let second_tuple = ("tokenB".to_string(), Decimal::from_ratio(1u128, 2u128));
    let msg = InstantiateMsg {
        addresses: vec![first_tupple.clone(), second_tuple.clone()],
        cw20_contracts: vec!["cw20_contract_one".to_string()],
    };

    let fee_splitter_instance = app
        .instantiate_contract(
            code_id,
            Addr::unchecked(sender),
            &msg,
            &[],
            "fee-splitter",
            None,
        )
        .unwrap();

    let config_response: Config = app
        .wrap()
        .query_wasm_smart(fee_splitter_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(config_response.addresses, vec![first_tupple, second_tuple]);
}

#[test]
#[should_panic(expected = "Provided weights exceed maximum allowed value")]
fn fails_to_init_because_weights_not_correct() {
    let mut app = CoreumApp::default();

    let code_id = store_fee_splitter_code(&mut app);
    let sender = "addr0000";

    let first_tupple = ("tokenA".to_string(), Decimal::from_ratio(2u128, 1u128));
    let second_tuple = ("tokenB".to_string(), Decimal::from_ratio(2u128, 1u128));
    let msg = InstantiateMsg {
        addresses: vec![first_tupple.clone(), second_tuple.clone()],
        cw20_contracts: vec!["cw20_contract_one".to_string()],
    };

    app.instantiate_contract(
        code_id,
        Addr::unchecked(sender),
        &msg,
        &[],
        "fee-splitter",
        None,
    )
    .unwrap();
}

#[test]
fn should_send_tokens_in_correct_amount() {
    let mut app = CoreumApp::default();

    let code_id = store_fee_splitter_code(&mut app);
    let sender = "addr0000";

    let first_tupple = ("tokenA".to_string(), Decimal::from_ratio(1u128, 2u128));
    let second_tuple = ("tokenB".to_string(), Decimal::from_ratio(1u128, 2u128));
    let msg = InstantiateMsg {
        addresses: vec![first_tupple.clone(), second_tuple.clone()],
        cw20_contracts: vec!["cw20_contract_one".to_string()],
    };

    let _ = app
        .instantiate_contract(
            code_id,
            Addr::unchecked(sender),
            &msg,
            &[],
            "fee-splitter",
            None,
        )
        .unwrap();

    let deps = mock_coreum_deps();
    let env = mock_env();
    let msg = ExecuteMsg::SendTokens {
        native_denoms: vec!["addr0000".to_string(), "addr0001".to_string()],
        cw20_addresses: vec!["cw20_contract_one".to_string()],
    };

    let res = execute(deps.as_ref(), env, msg).unwrap();
    ///todo I need to mock more things, start with the query_config
    dbg!(res);
}

fn store_fee_splitter_code(app: &mut CoreumApp) -> u64 {
    let fee_splitter_contract = Box::new(ContractWrapper::new(
        crate::contract::instantiate,
        crate::contract::instantiate,
        crate::contract::query,
    ));

    app.store_code(fee_splitter_contract)
}
