use cosmwasm_std::{
    testing::{mock_dependencies, mock_env, mock_info},
    Decimal,
};

use crate::{
    contract::{instantiate, CONFIG},
    msg::InstantiateMsg,
};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        addresses: vec![
            ("addr0000".to_string(), Decimal::from_ratio(1, 2)),
            ("addr0001".to_string(), Decimal::from_ratio(1, 2)),
        ],
        cw20_contracts: vec!["addr0002".to_string(), "addr0003".to_string()],
    };

    let sender = "addr1111";
    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env, info, msg);
    assert_eq!(res.messages, vec![]);

    let addresses = CONFIG.load(deps.as_ref().storage).unwrap().addresses;
    assert_eq!("contract-name", pool_info.liquidity_token);
    assert_eq!(pool_info.asset_infos, []);
}
