use std::{
    cmp::max,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use anyhow::{bail, Result as AnyResult};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use coreum_wasm_sdk::{
    assetft,
    core::{CoreumMsg, CoreumQueries},
};
use cosmwasm_std::{
    testing::{MockApi, MockStorage},
    to_json_binary, Addr, Api, BalanceResponse, BankMsg, BankQuery, Binary, BlockInfo, CustomQuery,
    Empty, Querier, QuerierWrapper, QueryRequest, Storage,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, CosmosRouter, Module, WasmKeeper,
};

/// How many seconds per block
/// (when we increment block.height, use this multiplier for block.time)
pub const BLOCK_TIME: u64 = 5;

pub struct CoreumModule {}

impl Module for CoreumModule {
    type ExecT = CoreumMsg;
    type QueryT = CoreumQueries;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: CoreumMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg {
            CoreumMsg::AssetFT(msg) => match msg {
                // Just return empty response for now, issue does nothing in mock
                assetft::Msg::Issue { .. } => Ok(AppResponse::default()),
                assetft::Msg::Mint { coin } => {
                    let mint_msg = BankSudo::Mint {
                        to_address: sender.to_string(),
                        amount: vec![coin],
                    };
                    router.sudo(api, storage, block, mint_msg.into())
                }
                // Also do nothing for now
                assetft::Msg::Burn { coin } => {
                    let burn_msg = BankMsg::Burn { amount: vec![coin] };
                    router.execute(api, storage, block, sender, burn_msg.into())
                }
                _ => bail!("Unsupported assetft message!"),
            },
            _ => bail!("Unsupported CoreumMsg execute!"),
        }
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        querier: &dyn Querier,
        _block: &BlockInfo,
        request: CoreumQueries,
    ) -> AnyResult<Binary> {
        match request {
            CoreumQueries::AssetFT(r) => match r {
                assetft::Query::Balance { account, denom } => {
                    let bank_query: QueryRequest<cosmwasm_std::Empty> =
                        QueryRequest::Bank(BankQuery::Balance {
                            address: account,
                            denom,
                        });
                    let res: BalanceResponse = QuerierWrapper::new(querier).query(&bank_query)?;
                    Ok(to_json_binary(&assetft::BalanceResponse {
                        balance: res.amount.amount.to_string(),
                        whitelisted: "".to_owned(),
                        frozen: "".to_owned(),
                        locked: "".to_owned(),
                    })?)
                }
                _ => bail!("Unsupported assetft query!"),
            },
            _ => bail!("Unsupported CoreumQueries query!"),
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        bail!("sudo not implemented for CoreumModule")
    }
}

pub type CoreumAppWrapped =
    App<BankKeeper, MockApi, MockStorage, CoreumModule, WasmKeeper<CoreumMsg, CoreumQueries>>;

pub struct CoreumApp(CoreumAppWrapped);

impl Deref for CoreumApp {
    type Target = CoreumAppWrapped;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CoreumApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for CoreumApp {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreumApp {
    pub fn new() -> Self {
        Self(
            BasicAppBuilder::<CoreumMsg, CoreumQueries>::new_custom()
                .with_custom(CoreumModule {})
                .build(|_router, _, _storage| ()),
        )
    }

    pub fn block_info(&self) -> BlockInfo {
        self.0.block_info()
    }

    /// This advances BlockInfo by given number of blocks.
    /// It does not do any callbacks, but keeps the ratio of seconds/blokc
    pub fn advance_blocks(&mut self, blocks: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(BLOCK_TIME * blocks);
            block.height += blocks;
        });
    }

    /// This advances BlockInfo by given number of seconds.
    /// It does not do any callbacks, but keeps the ratio of seconds/blokc
    pub fn advance_seconds(&mut self, seconds: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(seconds);
            block.height += max(1, seconds / BLOCK_TIME);
        });
    }
}
