use std::{collections::HashMap, marker::PhantomData};

use coreum_wasm_sdk::core::CoreumQueries;
use cosmwasm_std::{
    from_json,
    testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR},
    to_json_binary, Addr, Coin, Decimal, OwnedDeps, Querier, QuerierResult, QueryRequest,
    SystemError, SystemResult, Uint128, WasmQuery,
};
use dex::factory::{
    ConfigResponse, FeeInfoResponse,
    QueryMsg::{Config, FeeInfo},
};

use cw20::{BalanceResponse, Cw20QueryMsg, TokenInfoResponse};

pub type CoreumDeps = OwnedDeps<MockStorage, MockApi, SplitterMockQuerier, CoreumQueries>;

pub fn mock_coreum_deps(contract_balance: &[Coin]) -> CoreumDeps {
    let custom_qurier =
        SplitterMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    CoreumDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_qurier,
        custom_query_type: PhantomData,
    }
}

pub struct SplitterMockQuerier {
    base: MockQuerier<CoreumQueries>,
    token_querier: TokenQuerier,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // This lets us iterate over all pools that match the first string
    balances: HashMap<String, HashMap<String, Uint128>>,
}

impl TokenQuerier {
    pub fn new(balances: &[(&String, &[(&String, &Uint128)])]) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&String, &[(&String, &Uint128)])],
) -> HashMap<String, HashMap<String, Uint128>> {
    let mut balances_map: HashMap<String, HashMap<String, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<String, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(addr.to_string(), **balance);
        }

        balances_map.insert(contract_addr.to_string(), contract_balances_map);
    }
    balances_map
}
impl Querier for SplitterMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely
        let request: QueryRequest<CoreumQueries> = match from_json(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl SplitterMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<CoreumQueries>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr == "factory" {
                    match from_json(msg).unwrap() {
                        FeeInfo { .. } => SystemResult::Ok(
                            to_json_binary(&FeeInfoResponse {
                                fee_address: Some(Addr::unchecked("fee_address")),
                                total_fee_bps: 30,
                                protocol_fee_bps: 1660,
                            })
                            .into(),
                        ),
                        Config {} => SystemResult::Ok(
                            to_json_binary(&ConfigResponse {
                                owner: Addr::unchecked("owner"),
                                pool_configs: vec![],
                                fee_address: Some(Addr::unchecked("fee_address")),
                                max_referral_commission: Decimal::one(),
                                only_owner_can_create_pools: true,
                                trading_starts: None,
                            })
                            .into(),
                        ),
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    match from_json(msg).unwrap() {
                        Cw20QueryMsg::TokenInfo {} => {
                            let balances: &HashMap<String, Uint128> =
                                match self.token_querier.balances.get(contract_addr) {
                                    Some(balances) => balances,
                                    None => {
                                        return SystemResult::Err(SystemError::Unknown {});
                                    }
                                };

                            let mut total_supply = Uint128::zero();

                            for balance in balances {
                                total_supply += *balance.1;
                            }

                            SystemResult::Ok(
                                to_json_binary(&TokenInfoResponse {
                                    name: "mAPPL".to_string(),
                                    symbol: "mAPPL".to_string(),
                                    decimals: 6,
                                    total_supply,
                                })
                                .into(),
                            )
                        }
                        Cw20QueryMsg::Balance { address } => {
                            let balances: &HashMap<String, Uint128> =
                                match self.token_querier.balances.get(contract_addr) {
                                    Some(balances) => balances,
                                    None => {
                                        return SystemResult::Err(SystemError::Unknown {});
                                    }
                                };

                            let balance = match balances.get(&address) {
                                Some(v) => v,
                                None => {
                                    return SystemResult::Err(SystemError::Unknown {});
                                }
                            };

                            SystemResult::Ok(
                                to_json_binary(&BalanceResponse { balance: *balance }).into(),
                            )
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, .. }) => {
                if contract_addr == "factory" {
                    SystemResult::Ok(to_json_binary(&Vec::<Addr>::new()).into())
                } else {
                    panic!("DO NOT ENTER HERE");
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl SplitterMockQuerier {
    pub fn new(base: MockQuerier<CoreumQueries>) -> Self {
        SplitterMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
        }
    }

    // Configure the mint whitelist mock querier
    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances);
    }

    pub fn with_balance(&mut self, balances: &[(&String, &[Coin])]) {
        for (addr, balance) in balances {
            self.base.update_balance(addr.to_string(), balance.to_vec());
        }
    }
}
