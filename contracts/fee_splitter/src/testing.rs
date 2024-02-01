use bindings_test::{mock_coreum_deps, CoreumApp};
use coreum_wasm_sdk::core::CoreumMsg;
use cosmwasm_std::{
    testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR},
    Addr, Attribute, BankMsg, Coin, CosmosMsg, Decimal, ReplyOn, Uint128,
};
use cw_multi_test::{ContractWrapper, Executor};

use crate::{
    contract::{execute, instantiate},
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::Config,
};

pub type SubMsg = cosmwasm_std::SubMsg<CoreumMsg>;

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
    let mut deps = mock_coreum_deps(&[]);

    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[
            Coin {
                denom: "ATOM".to_string(),
                amount: Uint128::new(100_000000000000000000),
            },
            Coin {
                denom: "TIA".to_string(),
                amount: Uint128::new(100_000000000000000000),
            },
        ],
    )]);

    let env = mock_env();

    let sender = "addr0000";

    let info = mock_info(sender, &[]);
    let msg = InstantiateMsg {
        addresses: vec![
            ("address0000".to_string(), Decimal::percent(60u64)),
            ("address0001".to_string(), Decimal::percent(40u64)),
        ],
        cw20_contracts: vec![],
    };

    let fee_splitter_instance = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        fee_splitter_instance.attributes,
        vec![Attribute {
            key: "initialized".to_string(),
            value: "fee_splitter contract".to_string(),
        }]
    );

    let msg = ExecuteMsg::SendTokens {
        native_denoms: vec!["ATOM".to_string(), "TIA".to_string()],
        cw20_addresses: vec!["cw20_contract_one".to_string()],
    };

    let res = execute(deps.as_ref(), env, msg).unwrap();

    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                id: 0,
                msg: CosmosMsg::Bank(BankMsg::Send {
                    to_address: "address0000".to_string(),
                    amount: vec![
                        Coin {
                            denom: "ATOM".to_string(),
                            amount: Uint128::new(60000000000000000000),
                        },
                        Coin {
                            denom: "TIA".to_string(),
                            amount: Uint128::new(60000000000000000000),
                        }
                    ]
                }),
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                id: 0,
                msg: CosmosMsg::Bank(BankMsg::Send {
                    to_address: "address0001".to_string(),
                    amount: vec![
                        Coin {
                            denom: "ATOM".to_string(),
                            amount: Uint128::new(40000000000000000000),
                        },
                        Coin {
                            denom: "TIA".to_string(),
                            amount: Uint128::new(40000000000000000000),
                        }
                    ]
                }),
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
        ]
    );
}

fn store_fee_splitter_code(app: &mut CoreumApp) -> u64 {
    let fee_splitter_contract = Box::new(ContractWrapper::new(
        crate::contract::instantiate,
        crate::contract::instantiate,
        crate::contract::query,
    ));

    app.store_code(fee_splitter_contract)
}
