// src/slashing.rs
//! Automated state flag mutation utility for validator slashing.
//!
//! This module introduces a simple slashing mechanism that marks validator nodes
//! as `Status::Jailed` when they miss telemetry (heartbeat) checkpoints for a
//! strict ceiling of 100 ledger blocks. The slashing state is stored separately
//! from the existing `NodeProfile` to respect the requirement of not modifying
//! other parts of the codebase.

use soroban_sdk::{
    contracttype, env::Env, Address, Symbol, BytesN, Map,
};

/// Status of a validator node in the slashing subsystem.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The node is operating normally.
    Active,
    /// The node has been jailed due to prolonged inactivity.
    Jailed,
}

/// Persistent storage key for node slashing status mapping.
const SLASHED_NODES_KEY: Symbol = Symbol::short("SLASHED");

/// Ledger block interval threshold for slashing (100 blocks).
const SLASH_THRESHOLD_BLOCKS: u64 = 100;

/// Record slashing status for a validator node.
///
/// This function checks the last heartbeat timestamp for the given `asset`
/// (representing the telemetry feed associated with the validator). If the
/// difference between the current ledger timestamp and the last recorded
/// heartbeat exceeds `SLASH_THRESHOLD_BLOCKS * heartbeat_interval`, the node is
/// marked as `Status::Jailed`.
pub fn maybe_slash_node(env: &Env, node: &Address, asset: BytesN<32>) {
    // Retrieve the last heartbeat timestamp for the asset.
    let last_ts_opt: Option<u64> = super::TimeLockedUpgradeContract::get_last_update_timestamp(
        env.clone(),
        // Convert BytesN to AssetId (u32) – the contract uses `AssetId = u32`.
        // For simplicity we assume the asset identifier fits within a u32.
        // In real usage the caller should provide the appropriate AssetId.
        // Here we truncate the BytesN to u32 via little‑endian conversion.
        u32::from_le_bytes(asset.slice(0..4).try_into().unwrap_or([0; 4])),
    );

    // Determine the heartbeat interval configured for the contract.
    let interval = super::TimeLockedUpgradeContract::get_heartbeat_interval(env.clone());
    let current_ts = env.ledger().timestamp();

    let should_jail = match last_ts_opt {
        Some(last_ts) => {
            // Calculate how many ledger blocks have passed since the last
            // heartbeat. The contract does not expose block numbers directly,
            // but the heartbeat interval (in seconds) can approximate blocks.
            // We treat a "block" as one heartbeat interval.
            let elapsed = current_ts.saturating_sub(last_ts);
            // Convert elapsed seconds to block count.
            let blocks_missed = if interval == 0 { 0 } else { elapsed / interval };
            blocks_missed >= SLASH_THRESHOLD_BLOCKS
        }
        None => true, // No heartbeat at all ⇒ definitely jail.
    };

    if should_jail {
        // Update the persistent map with the jailed status.
        let mut status_map: Map<Address, Status> =
            env.storage().persistent().get(&SLASHED_NODES_KEY).unwrap_or_else(|| Map::new(env));
        status_map.set(node.clone(), Status::Jailed);
        env.storage().persistent().set(&SLASHED_NODES_KEY, &status_map);
    }
}

/// Retrieve the slashing status of a node.
pub fn get_node_status(env: &Env, node: &Address) -> Status {
    let map: Map<Address, Status> =
        env.storage().persistent().get(&SLASHED_NODES_KEY).unwrap_or_else(|| Map::new(env));
    map.get(node.clone()).unwrap_or(Status::Active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, testutils::Address as _, testutils::Ledger, Symbol};

    #[test]
    fn test_jail_when_no_heartbeat() {
        let env = Env::default();
        env.mock_all_auths();
        let node = Address::generate(&env);
        let asset_bytes = BytesN::from_array(&env, &[0; 32]);
        maybe_slash_node(&env, &node, asset_bytes);
        assert_eq!(get_node_status(&env, &node), Status::Jailed);
    }
}
