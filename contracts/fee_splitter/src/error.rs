use cosmwasm_std::StdError;
use thiserror::Error;

/// This enum describes factory contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Provided weights exceed maximum allowed value")]
    InvalidWeights {},

    #[error("Unauthorized")]
    Unauthorized {},
}
