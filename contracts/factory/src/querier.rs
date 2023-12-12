use coreum_wasm_sdk::core::CoreumQueries;
use cosmwasm_std::{QuerierWrapper, StdResult};

use dex::pool::{PairInfo, QueryMsg};

/// Returns information about a pair (using the [`PoolInfo`] struct).
///
/// `pool_contract` is the pool for which to retrieve information.
pub fn query_pair_info(
    querier: &QuerierWrapper<CoreumQueries>,
    pool_contract: impl Into<String>,
) -> StdResult<PairInfo> {
    querier.query_wasm_smart(pool_contract, &QueryMsg::Pair {})
}
