use std::marker::PhantomData;

use coreum_wasm_sdk::core::{CoreumMsg, CoreumQueries};
use cosmwasm_std::{
    testing::{MockApi, MockQuerier, MockStorage},
    Addr, Api, Binary, BlockInfo, Coin, CustomQuery, Empty, Order, OwnedDeps, Querier,
    QuerierResult, StdError, StdResult, Storage, Timestamp,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, Executor, Module,
    WasmKeeper,
};

pub struct CoreumModule {}

/// How many seconds per block
/// (when we increment block.height, use this multiplier for block.time)
pub const BLOCK_TIME: u64 = 5;

pub type CoreumDeps = OwnedDeps<MockStorage, MockApi, MockQuerier, CoreumQueries>;

pub fn mock_coreum_deps() -> CoreumDeps {
    CoreumDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: MockQuerier::default(),
        custom_query_type: PhantomData,
    }
}

pub type CoreumAppWrapped =
    App<BankKeeper, MockApi, MockStorage, WasmKeeper<CoreumMsg, CoreumQueries>>;
