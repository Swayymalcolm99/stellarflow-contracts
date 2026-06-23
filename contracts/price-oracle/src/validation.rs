//! Shared ref-based helpers for vector-heavy public methods.
//!
//! These helpers borrow Soroban `Vec` inputs so large payloads stay behind a
//! shared immutable reference while the contract performs validation and batch
//! processing.

use soroban_sdk::{Env, Symbol, Vec};

use crate::{
    types::{AssetRegistrationConfig, AssetWeight, DataKey},
    ContractError, MAX_CLEAR_ASSETS,
};

/// Validate a batch of asset registration configs without taking ownership of
/// the vector payload.
pub fn validate_asset_registration_configs(
    configs: &Vec<AssetRegistrationConfig>,
    max_deviation_bps: i128,
) -> Result<(), ContractError> {
    if configs.len() == 0 {
        return Err(ContractError::InvalidAssetConfig);
    }

    if max_deviation_bps <= 0 || max_deviation_bps > 10_000 {
        return Err(ContractError::InvalidMaxDeviation);
    }

    for config in configs.iter() {
        if config.min_price <= 0 || config.max_price <= 0 || config.min_price > config.max_price {
            return Err(ContractError::InvalidPriceBounds);
        }

        if let Some(price_floor) = config.price_floor {
            if price_floor <= 0 || price_floor > config.max_price {
                return Err(ContractError::InvalidPriceBounds);
            }
        }
    }

    Ok(())
}

/// Compute the weighted index price from a borrowed basket of assets.
pub fn calculate_index_price(
    env: &Env,
    components: &Vec<AssetWeight>,
) -> Result<i128, ContractError> {
    if components.is_empty() {
        return Err(ContractError::AssetNotFound);
    }

    let mut total_weighted_price: i128 = 0;
    let mut total_weight: u32 = 0;

    for component in components.iter() {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::TrackedAsset(component.asset.clone()))
        {
            return Err(ContractError::AssetNotFound);
        }

        if component.weight == 0 {
            return Err(ContractError::InvalidWeight);
        }

        let price_data = crate::PriceOracle::get_price(env.clone(), component.asset.clone(), true)?;
        let weight_i128: i128 = component.weight.into();
        let weighted_val = price_data
            .price
            .checked_mul(weight_i128)
            .ok_or(ContractError::InvalidPrice)?;

        total_weighted_price = total_weighted_price
            .checked_add(weighted_val)
            .ok_or(ContractError::InvalidPrice)?;

        total_weight = total_weight
            .checked_add(component.weight)
            .unwrap_or(total_weight);
    }

    if total_weight == 0 {
        return Err(ContractError::InvalidWeight);
    }

    total_weighted_price
        .checked_div(total_weight as i128)
        .ok_or(ContractError::PriceMathOverflow)
}

/// Remove a batch of price entries without copying the input vector.
pub fn clear_assets(env: &Env, assets: &Vec<Symbol>) -> Result<(), ContractError> {
    if assets.len() > MAX_CLEAR_ASSETS {
        return Err(ContractError::TooManyAssets);
    }

    let storage = env.storage().persistent();
    for asset in assets.iter() {
        storage.remove(&DataKey::Price(asset));
    }

    Ok(())
}
