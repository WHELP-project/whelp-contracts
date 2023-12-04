use cosmwasm_std::{QuerierWrapper, StdResult};
use dex::pool::{PoolInfo, QueryMsg};

/// Returns information about a pair (using the [`PoolInfo`] struct).
///
/// `pool_contract` is the pool for which to retrieve information.
pub fn query_pair_info(
    querier: &QuerierWrapper,
    pair_contract: impl Into<String>,
) -> StdResult<PoolInfo> {
    querier.query_wasm_smart(pool_contract, &QueryMsg::Pair {})
}
