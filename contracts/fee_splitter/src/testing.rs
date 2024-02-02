use bindings_test::mock_coreum_deps;
use cosmwasm_std::{
    from_json,
    testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR},
    Attribute, BankMsg, Coin, CosmosMsg, Decimal, ReplyOn, Uint128,
};

use crate::{
    contract::{execute, instantiate, query, SubMsg},
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::Config,
};

#[test]
fn init_works() {
    let mut deps = mock_coreum_deps(&[]);
    let env = mock_env();
    let sender = "addr0000";
    let info = mock_info(sender, &[]);

    let first_addr_pecnt = ("address0000".to_string(), Decimal::percent(50u64));
    let second_addr_pecnt = ("address0001".to_string(), Decimal::percent(50u64));
    let msg = InstantiateMsg {
        addresses: vec![first_addr_pecnt.clone(), second_addr_pecnt.clone()],
        cw20_contracts: vec!["USDT".to_string()],
    };

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        res.attributes,
        vec![Attribute {
            key: "initialized".to_string(),
            value: "fee_splitter contract".to_string(),
        }]
    );
}

#[test]
fn fails_to_init_because_weights_not_correct() {
    let mut deps = mock_coreum_deps(&[]);
    let env = mock_env();
    let sender = "addr0000";
    let info = mock_info(sender, &[]);

    let first_tupple = ("ATOM".to_string(), Decimal::percent(50u64));
    let second_tuple = ("TIA".to_string(), Decimal::percent(60u64));
    let msg = InstantiateMsg {
        addresses: vec![first_tupple.clone(), second_tuple.clone()],
        cw20_contracts: vec!["USDT".to_string()],
    };

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::InvalidWeights {});
}

#[test]
fn should_send_tokens_in_correct_amount() {
    let mut deps = mock_coreum_deps(&[]);

    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[
            Coin {
                denom: "ATOM".to_string(),
                amount: Uint128::new(100_000),
            },
            Coin {
                denom: "TIA".to_string(),
                amount: Uint128::new(100_000),
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

    let fee_splitter_instance = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
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

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

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
                            amount: Uint128::new(60_000),
                        },
                        Coin {
                            denom: "TIA".to_string(),
                            amount: Uint128::new(60_000),
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
                            amount: Uint128::new(40_000),
                        },
                        Coin {
                            denom: "TIA".to_string(),
                            amount: Uint128::new(40_000),
                        }
                    ]
                }),
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
        ]
    );

    let msg = QueryMsg::Config {};

    let query_result = query(deps.as_ref(), env, msg).unwrap();
    let config_res: Config = from_json(query_result).unwrap();
    assert_eq!(
        config_res,
        Config {
            addresses: vec![
                ("address0000".to_string(), Decimal::percent(60)),
                ("address0001".to_string(), Decimal::percent(40))
            ],
        }
    );
}
