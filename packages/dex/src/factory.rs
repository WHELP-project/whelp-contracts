use crate::{
    asset::{Asset, AssetInfo},
    fee_config::FeeConfig,
    pool::{PairInfo, StakeConfig},
    stake::UnbondingPeriod,
};

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use cw_storage_plus::Map;
use std::fmt::{Display, Formatter, Result};

/// This enum describes available pool types.
/// ## Available pool types
/// ```
/// # use dex::factory::PoolType::{Custom, Stable, Xyk};
/// Xyk {};
/// Stable {};
/// Custom(String::from("Custom"));
/// ```
#[cw_serde]
pub enum PoolType {
    /// XYK pool type
    Xyk {},
    /// Stable pool type
    Stable {},
    /// Custom pool type
    Custom(String),
}

/// Returns a raw encoded string representing the name of each pool type
impl Display for PoolType {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        match self {
            PoolType::Xyk {} => fmt.write_str("xyk"),
            PoolType::Stable {} => fmt.write_str("stable"),
            PoolType::Custom(pool_type) => fmt.write_str(format!("custom-{}", pool_type).as_str()),
        }
    }
}

/// This structure stores a pool type's configuration.
#[cw_serde]
pub struct PoolConfig {
    /// ID of contract which is allowed to create pools of this type
    pub code_id: u64,
    /// The pool type (provided in a [`PoolType`])
    pub pool_type: PoolType,
    /// The default fee configuration for this pool type. Total fee be overridden when creating a pool.
    pub fee_config: FeeConfig,
    /// Whether a pool type is disabled or not. If it is disabled, new pools cannot be
    /// created, but existing ones can still read the pool configuration
    pub is_disabled: bool,
}

/// This structure stores the basic settings for creating a new factory contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// IDs of contracts that are allowed to instantiate pools
    pub pool_configs: Vec<PoolConfig>,
    /// Contract address to send governance fees to (the protocol).
    /// If this is not specified, no protocol fees are paid out regardless of the fee configuration
    pub fee_address: Option<String>,
    /// Address of owner that is allowed to change factory contract parameters.
    pub owner: String,
    /// Maximum referral commission
    pub max_referral_commission: Decimal,
    /// Default values for lp token staking contracts
    pub default_stake_config: DefaultStakeConfig,
    /// The block time until which trading is disabled
    pub trading_starts: Option<u64>,
    /// Fee required to create non-verified pool
    pub pool_creation_fee: Asset,
}

#[cw_serde]
pub struct DefaultStakeConfig {
    /// The staking contract code ID
    pub staking_code_id: u64,
    pub tokens_per_power: Uint128,
    pub min_bond: Uint128,
    pub unbonding_periods: Vec<u64>,
    pub max_distributions: u32,
}

impl DefaultStakeConfig {
    pub fn combine_with(mut self, partial: PartialStakeConfig) -> Self {
        if let Some(staking_code_id) = partial.staking_code_id {
            self.staking_code_id = staking_code_id;
        }
        if let Some(tokens_per_power) = partial.tokens_per_power {
            self.tokens_per_power = tokens_per_power;
        }
        if let Some(min_bond) = partial.min_bond {
            self.min_bond = min_bond;
        }
        if let Some(unbonding_periods) = partial.unbonding_periods {
            self.unbonding_periods = unbonding_periods;
        }
        if let Some(max_distributions) = partial.max_distributions {
            self.max_distributions = max_distributions;
        }

        self
    }

    pub fn update(&mut self, partial: PartialDefaultStakeConfig) {
        if let Some(staking_code_id) = partial.staking_code_id {
            self.staking_code_id = staking_code_id;
        }
        if let Some(tokens_per_power) = partial.tokens_per_power {
            self.tokens_per_power = tokens_per_power;
        }
        if let Some(min_bond) = partial.min_bond {
            self.min_bond = min_bond;
        }
        if let Some(unbonding_periods) = partial.unbonding_periods {
            self.unbonding_periods = unbonding_periods;
        }
        if let Some(max_distributions) = partial.max_distributions {
            self.max_distributions = max_distributions;
        }
    }

    pub fn to_stake_config(self) -> StakeConfig {
        StakeConfig {
            staking_code_id: self.staking_code_id,
            tokens_per_power: self.tokens_per_power,
            min_bond: self.min_bond,
            unbonding_periods: self.unbonding_periods,
            max_distributions: self.max_distributions,
        }
    }
}

/// For docs, see [`DefaultStakeConfig`]
#[cw_serde]
pub struct PartialDefaultStakeConfig {
    pub staking_code_id: Option<u64>,
    pub tokens_per_power: Option<Uint128>,
    pub min_bond: Option<Uint128>,
    pub unbonding_periods: Option<Vec<u64>>,
    pub max_distributions: Option<u32>,
}

/// This structure describes the execute messages of the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// UpdateConfig updates relevant code IDs
    UpdateConfig {
        /// Contract address to send governance fees to (the protocol)
        fee_address: Option<String>,
        /// Whether only the owner or anyone can create new pools
        only_owner_can_create_pools: Option<bool>,
        /// The default configuration for the staking contracts of new pools
        default_stake_config: Option<PartialDefaultStakeConfig>,
    },
    /// UpdatePoolConfig updates the config for a pool type.
    UpdatePoolConfig {
        /// New [`PoolConfig`] settings for a pool type
        config: PoolConfig,
    },
    /// CreatePool instantiates a new pool contract.
    CreatePool {
        /// The pool type (exposed in [`PoolType`])
        pool_type: PoolType,
        /// The assets to create the pool for
        asset_infos: Vec<AssetInfo>,
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
        /// The total fees (in bps) charged by a pool of this type.
        /// In relation to the returned amount of tokens.
        /// If not provided, the default is used.
        total_fee_bps: Option<u16>,
        /// Config for the staking contract
        #[serde(default)]
        staking_config: PartialStakeConfig,
    },
    /// UpdatePoolFees updates the fees for a pool.
    /// This just sends the corresponding message to the pool.
    UpdatePoolFees {
        /// The pool to update
        asset_infos: Vec<AssetInfo>,
        /// The new fee config
        fee_config: FeeConfig,
    },
    /// Deregister removes a previously created pool.
    Deregister {
        /// The assets for which we deregister a pool
        asset_infos: Vec<AssetInfo>,
    },
    /// ProposeNewOwner creates a proposal to change contract ownership.
    /// The validity period for the proposal is set in the `expires_in` variable.
    ProposeNewOwner {
        /// Newly proposed contract owner
        owner: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the existing offer to change contract ownership.
    DropOwnershipProposal {},
    /// Used to claim contract ownership.
    ClaimOwnership {},
    /// MarkAsMigrated marks pools as migrated
    MarkAsMigrated { pools: Vec<String> },
    /// Combines pool creation and creation of distribution flows for the pool staking contract
    /// into one message
    CreatePoolAndDistributionFlows {
        /// The pool type (exposed in [`PoolType`])
        pool_type: PoolType,
        /// The assets to create the pool for
        asset_infos: Vec<AssetInfo>,
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
        /// The total fees (in bps) charged by a pool of this type.
        /// In relation to the returned amount of tokens.
        /// If not provided, the default is used.
        total_fee_bps: Option<u16>,
        /// Config for the staking contract
        #[serde(default)]
        staking_config: PartialStakeConfig,
        /// The distribution flows to create
        distribution_flows: Vec<DistributionFlow>,
    },
    /// Creates a distribution flow for the pool staking contract
    CreateDistributionFlow {
        /// The assets pool for which the distribution flow will be created
        asset_infos: Vec<AssetInfo>,
        /// The asset that will be distributed
        asset: AssetInfo,

        /// Rewards multiplier by unbonding period for this distribution
        /// Only periods that are defined in the contract can be used here
        rewards: Vec<(UnbondingPeriod, Decimal)>,
    },
    /// Withdraws pool fee creations to the owner of the contract
    WithdrawPoolCreationFees {},
    /// Implements the Cw20 receiver interface.
    Receive(Cw20ReceiveMsg),
}

#[cw_serde]
pub enum ReceiveMsg {
    /// CreatePool instantiates a new pool contract.
    CreatePool {
        /// The pool type (exposed in [`PoolType`])
        pool_type: PoolType,
        /// The assets to create the pool for
        asset_infos: Vec<AssetInfo>,
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
        /// The total fees (in bps) charged by a pool of this type.
        /// In relation to the returned amount of tokens.
        /// If not provided, the default is used.
        total_fee_bps: Option<u16>,
        /// Config for the staking contract
        #[serde(default)]
        staking_config: PartialStakeConfig,
    },
    CreatePoolAndDistributionFlows {
        /// The pool type (exposed in [`PoolType`])
        pool_type: PoolType,
        /// The assets to create the pool for
        asset_infos: Vec<AssetInfo>,
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
        /// The total fees (in bps) charged by a pool of this type.
        /// In relation to the returned amount of tokens.
        /// If not provided, the default is used.
        total_fee_bps: Option<u16>,
        /// Config for the staking contract
        #[serde(default)]
        staking_config: PartialStakeConfig,
        /// The distribution flows to create
        distribution_flows: Vec<DistributionFlow>,
    },
}

#[cw_serde]
pub struct DistributionFlow {
    /// The asset that will be distributed
    pub asset: AssetInfo,

    /// Rewards multiplier by unbonding period for this distribution
    /// Only periods that are defined in the contract can be used here
    pub rewards: Vec<(UnbondingPeriod, Decimal)>,
    /// The number of seconds over which funded distributions are stretched.
    pub reward_duration: u64,
}

/// Like [`StakeConfig`] but with all fields being optional.
#[cw_serde]
#[derive(Default)]
pub struct PartialStakeConfig {
    /// The staking contract code ID
    pub staking_code_id: Option<u64>,
    pub tokens_per_power: Option<Uint128>,
    pub min_bond: Option<Uint128>,
    pub unbonding_periods: Option<Vec<u64>>,
    pub max_distributions: Option<u32>,
}

/// This structure describes the available query messages for the factory contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Config returns contract settings specified in the custom [`ConfigResponse`] structure.
    #[returns(ConfigResponse)]
    Config {},
    /// Pool returns information about a specific pool according to the specified assets.
    #[returns(PairInfo)]
    Pool {
        /// The assets for which we return a pool
        asset_infos: Vec<AssetInfo>,
    },
    /// Pools returns an array of pools and their information according to the specified parameters in `start_after` and `limit` variables.
    #[returns(PoolsResponse)]
    Pools {
        /// The pool item to start reading from. It is an [`Option`] type that accepts [`AssetInfo`] elements.
        start_after: Option<Vec<AssetInfo>>,
        /// The number of pools to read and return. It is an [`Option`] type.
        limit: Option<u32>,
    },
    /// FeeInfo returns default fee parameters for a specific pool type.
    /// If you want to get the fee parameters for a specific pool, use the `Pool` query.
    /// The response is returned using a [`FeeInfoResponse`] structure
    #[returns(FeeInfoResponse)]
    FeeInfo {
        /// The pool type for which we return fee information. Pool type is a [`PoolType`] struct
        pool_type: PoolType,
    },
    /// Returns a vector that contains blacklisted pool types
    #[returns(Vec<PoolType>)]
    BlacklistedPoolTypes {},
    /// Returns a vector that contains pool addresses that are not migrated
    #[returns(Vec<Addr>)]
    PoolsToMigrate {},
    /// Returns true if the given address is an LP token staking contract
    /// Used by the `gauge-adapter` contract
    #[returns(bool)]
    ValidateStakingAddress { address: String },
}

/// A custom struct for each query response that returns general contract settings/configs.
#[cw_serde]
pub struct ConfigResponse {
    /// Addres of owner that is allowed to change contract parameters
    pub owner: Addr,
    /// IDs of contracts which are allowed to create pools
    pub pool_configs: Vec<PoolConfig>,
    /// Address of contract to send governance fees to (the protocol)
    pub fee_address: Option<Addr>,
    /// Maximum referral commission
    pub max_referral_commission: Decimal,
    /// When this is set to `true`, only the owner can create pools
    pub only_owner_can_create_pools: bool,
    /// The block time until which trading is disabled
    pub trading_starts: Option<u64>,
}

/// A custom struct for each query response that returns an array of objects of type [`PairInfo`].
#[cw_serde]
pub struct PoolsResponse {
    /// Arrays of structs containing information about multiple pools
    pub pools: Vec<PairInfo>,
}

/// A custom struct for each query response that returns an object of type [`FeeInfoResponse`].
#[cw_serde]
pub struct FeeInfoResponse {
    /// Contract address to send governance fees to
    pub fee_address: Option<Addr>,
    /// Total amount of fees (in bps) charged on a swap
    pub total_fee_bps: u16,
    /// Amount of fees (in bps) sent to the protocol
    pub protocol_fee_bps: u16,
}

/// This is an enum used for setting and removing a contract address.
#[cw_serde]
pub enum UpdateAddr {
    /// Sets a new contract address.
    Set(String),
    /// Removes a contract address.
    Remove {},
}

#[cw_serde]
#[allow(clippy::large_enum_variant)]
pub enum MigrateMsg {
    Update(),
    /// Required with <=2.1.0 migration
    AddPermissionlessPoolDeposit(Asset),
}

/// Map which contains a list of all pools which are able to convert X <> Y assets.
/// Example: given 3 pools (X, Y), (X,Y,Z) and (X,Y,Z,W), the map will contain the following entries
/// (pool addresses):
/// `ROUTE[X][Y] = [(X,Y), (X,Y,Z), (X,Y,Z,W)]`
/// `ROUTE[X][Z] = [(X,Y,Z), (X,Y,Z,W)]`
/// `ROUTE[X][W] = [(X,Y,Z,W)]`
/// ...
///
/// Notice that `ROUTE[X][Y] = ROUTE[Y][X]`
pub const ROUTE: Map<(String, String), Vec<Addr>> = Map::new("routes");
