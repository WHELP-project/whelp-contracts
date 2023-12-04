use cosmwasm_std::{Decimal, StdError, Uint128};
use thiserror::Error;

/// This enum describes factory contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Invalid value for trading start")]
    InvalidTradingStart {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Pair was already created")]
    PairWasCreated {},

    #[error("Pair was already registered")]
    PairWasRegistered {},

    #[error("Duplicate of pair configs")]
    PoolConfigDuplicate {},

    #[error("Fee bps in pair config must be smaller than or equal to 10,000")]
    PoolConfigInvalidFeeBps {},

    #[error("Pool config not found")]
    PoolConfigNotFound {},

    #[error("Pool config disabled")]
    PoolConfigDisabled {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("Invalid referral commision: {0}")]
    InvalidReferralCommission(Decimal),

    #[error("Can only init upgrade from cw-placeholder")]
    NotPlaceholder,

    #[error("Permissionless dex requires deposit to be set")]
    DepositNotSet {},

    #[error("Incorrect deposit: permissionless factory requires deposit as: {0}{1}")]
    DepositRequired(Uint128, String),

    #[error("Factory is in permissionless mode: deposit must be sent to create new pair")]
    PermissionlessRequiresDeposit {},
}
