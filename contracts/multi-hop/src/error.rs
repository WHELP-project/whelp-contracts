use thiserror::Error;

use cosmwasm_std::{OverflowError, StdError, Uint128};

/// This enum describes oracle contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Must specify swap operations!")]
    MustProvideOperations {},

    #[error("Assertion failed; minimum receive amount: {receive}, swap amount: {amount}")]
    AssertionMinimumReceive { receive: Uint128, amount: Uint128 },

    #[error("The swap operation limit was exceeded!")]
    SwapLimitExceeded {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
