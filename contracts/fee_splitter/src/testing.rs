use bindings_test::mock_coreum_deps;
use cosmwasm_std::{
    from_json,
    testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR},
    to_json_binary, Attribute, BankMsg, Coin, CosmosMsg, Decimal, ReplyOn, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use crate::{
    contract::{execute, instantiate, query, SubMsg},
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    state::Config,
};

const SENDER: &str = "addr0000";
const FIRST_RECIPIENT: &str = "address0000";
const SECOND_RECIPIENT: &str = "address0001";
const ATOM: &str = "ATOM";
const TIA: &str = "TIA";
const USDT: &str = "USDT";
const CW20_ASSET_ONE: &str = "asset0000";
const CW20_ASSET_TWO: &str = "asset0001";

#[test]
fn init_works() {
    let mut deps = mock_coreum_deps(&[]);
    let env = mock_env();
    let info = mock_info(SENDER, &[]);

    let first_addr_percent = (FIRST_RECIPIENT.to_string(), Decimal::percent(50u64));
    let second_addr_percent = (SECOND_RECIPIENT.to_string(), Decimal::percent(50u64));
    let msg = InstantiateMsg {
        addresses: vec![first_addr_percent.clone(), second_addr_percent.clone()],
        cw20_contracts: vec![USDT.to_string()],
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
fn fails_to_init_because_weights_above_limit() {
    let mut deps = mock_coreum_deps(&[]);
    let env = mock_env();
    let info = mock_info(SENDER, &[]);

    let first_addr_percent = (FIRST_RECIPIENT.to_string(), Decimal::percent(50u64));
    let second_addr_percent = (SECOND_RECIPIENT.to_string(), Decimal::percent(60u64));
    let msg = InstantiateMsg {
        addresses: vec![first_addr_percent.clone(), second_addr_percent.clone()],
        cw20_contracts: vec![USDT.to_string()],
    };

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::InvalidWeights {});
}

#[test]
fn fails_to_init_because_weights_below_limit() {
    let mut deps = mock_coreum_deps(&[]);
    let env = mock_env();
    let info = mock_info(SENDER, &[]);

    let first_addr_percent = (FIRST_RECIPIENT.to_string(), Decimal::percent(20u64));
    let second_addr_percent = (SECOND_RECIPIENT.to_string(), Decimal::percent(20u64));
    let msg = InstantiateMsg {
        addresses: vec![first_addr_percent.clone(), second_addr_percent.clone()],
        cw20_contracts: vec![USDT.to_string()],
    };

    let res = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::InvalidWeights {});
}

#[test]
fn should_send_tokens_in_correct_amount() {
    let mut deps = mock_coreum_deps(&[]);

    deps.querier.with_token_balances(&[(
        &String::from(CW20_ASSET_ONE),
        &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(100_000))],
    )]);

    deps.querier.with_balance(&[(
        &String::from(MOCK_CONTRACT_ADDR),
        &[
            Coin {
                denom: ATOM.to_string(),
                amount: Uint128::new(100_000),
            },
            Coin {
                denom: TIA.to_string(),
                amount: Uint128::new(100_000),
            },
        ],
    )]);

    let env = mock_env();

    let info = mock_info(SENDER, &[]);
    let msg = InstantiateMsg {
        addresses: vec![
            (FIRST_RECIPIENT.to_string(), Decimal::percent(60u64)),
            (SECOND_RECIPIENT.to_string(), Decimal::percent(40u64)),
        ],
        cw20_contracts: vec![CW20_ASSET_ONE.to_string(), CW20_ASSET_TWO.to_string()],
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
        native_denoms: vec![ATOM.to_string(), TIA.to_string()],
        cw20_addresses: vec![CW20_ASSET_ONE.to_string()],
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages,
        vec![
            SubMsg {
                id: 0,
                msg: CosmosMsg::Bank(BankMsg::Send {
                    to_address: FIRST_RECIPIENT.to_string(),
                    amount: vec![
                        Coin {
                            denom: ATOM.to_string(),
                            amount: Uint128::new(60_000),
                        },
                        Coin {
                            denom: TIA.to_string(),
                            amount: Uint128::new(60_000),
                        }
                    ]
                }),
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                id: 0,
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: CW20_ASSET_ONE.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: FIRST_RECIPIENT.to_string(),
                        amount: Uint128::new(60_000),
                    })
                    .unwrap(),
                    funds: vec![]
                }),
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                id: 0,
                msg: CosmosMsg::Bank(BankMsg::Send {
                    to_address: SECOND_RECIPIENT.to_string(),
                    amount: vec![
                        Coin {
                            denom: ATOM.to_string(),
                            amount: Uint128::new(40_000),
                        },
                        Coin {
                            denom: TIA.to_string(),
                            amount: Uint128::new(40_000),
                        }
                    ]
                }),
                gas_limit: None,
                reply_on: ReplyOn::Never
            },
            SubMsg {
                id: 0,
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: CW20_ASSET_ONE.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: SECOND_RECIPIENT.to_string(),
                        amount: Uint128::new(40_000),
                    })
                    .unwrap(),
                    funds: vec![]
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
                (FIRST_RECIPIENT.to_string(), Decimal::percent(60)),
                (SECOND_RECIPIENT.to_string(), Decimal::percent(40))
            ],
        }
    );
}
