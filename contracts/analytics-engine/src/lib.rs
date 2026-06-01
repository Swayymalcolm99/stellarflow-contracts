#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol};

const ALPHA_SCALE: i128 = 10_000;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    EmaRecord(Symbol), // Maps an asset symbol to its EMA
    Alpha,             // The smoothing factor
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct EmaRecord {
    pub value: i128,
    pub last_updated: u64,
}

#[contract]
pub struct AnalyticsEngine;

#[contractimpl]
impl AnalyticsEngine {
    pub fn initialize(env: Env, alpha: i128) {
        if env.storage().instance().has(&DataKey::Alpha) {
            panic!("already initialized");
        }
        if alpha <= 0 || alpha > ALPHA_SCALE {
            panic!("invalid alpha");
        }
        env.storage().instance().set(&DataKey::Alpha, &alpha);
    }

    /// Implement an optimized calculation method that updates a single, rolling smoothing metric upon every new price submission.
    /// Store only the finalized moving average record in persistent data slots to minimize long-term storage rent fees.
    pub fn submit_price(env: Env, asset: Symbol, price: i128) {
        if price <= 0 {
            panic!("price must be positive");
        }
        
        let alpha: i128 = env.storage().instance().get(&DataKey::Alpha).unwrap_or_else(|| panic!("not initialized"));
        let key = DataKey::EmaRecord(asset.clone());
        
        let new_ema = if let Some(record) = env.storage().persistent().get::<_, EmaRecord>(&key) {
            // Calculate new EMA: (Price * alpha + Old_EMA * (1 - alpha))
            (price * alpha + record.value * (ALPHA_SCALE - alpha)) / ALPHA_SCALE
        } else {
            // First price submission becomes the initial EMA
            price
        };

        let new_record = EmaRecord {
            value: new_ema,
            last_updated: env.ledger().timestamp(),
        };

        // Store only the finalized moving average record in persistent data slots
        env.storage().persistent().set(&key, &new_record);
    }

    pub fn get_ema(env: Env, asset: Symbol) -> i128 {
        let key = DataKey::EmaRecord(asset);
        if let Some(record) = env.storage().persistent().get::<_, EmaRecord>(&key) {
            record.value
        } else {
            0
        }
    }
}
