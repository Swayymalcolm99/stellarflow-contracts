use soroban_sdk::{contracttype, symbol_short, Address, Env, Map, Symbol};
use crate::{ContractData, ContractError, DATA_KEY, SIGNERS_KEY};

pub(crate) const PENDING_OWNER_KEY: Symbol = symbol_short!("PNDOWN");
pub(crate) const PENDING_ADMIN_KEY: Symbol = symbol_short!("PADMIN");

const ADMIN_CHANGE_TIMELOCK_SECONDS: u64 = 24 * 60 * 60;

#[contracttype]
#[derive(Clone)]
pub struct PendingOwner {
    pub nominee: Address,
    pub proposed_by: Address,
}

/// Pending two-phase admin key change record.
/// Cleared when either the cosigner approves (instant) or the timelock elapses.
#[contracttype]
#[derive(Clone)]
pub struct AdminChangeProposal {
    pub new_admin: Address,
    pub proposer: Address,
    pub proposed_at: u64,
}

// ─── Issue #429: Two-phase ownership transfer ────────────────────────────────

/// Phase 1: current admin nominates a new owner.
/// Stores the nominee under `PNDOWN`; does not transfer ownership yet.
pub fn propose_ownership_transfer(
    env: &Env,
    current_admin: Address,
    nominee: Address,
) -> Result<(), ContractError> {
    let data: ContractData = env
        .storage()
        .instance()
        .get(&DATA_KEY)
        .ok_or(ContractError::NotInitialized)?;

    if data.admin != current_admin {
        return Err(ContractError::NotAdmin);
    }
    current_admin.require_auth();

    if env.storage().instance().has(&PENDING_OWNER_KEY) {
        return Err(ContractError::TransferAlreadyPending);
    }

    env.storage().instance().set(
        &PENDING_OWNER_KEY,
        &PendingOwner {
            nominee,
            proposed_by: current_admin,
        },
    );
    Ok(())
}

/// Phase 2: nominee claims ownership, proving key access.
/// Only succeeds when a pending transfer exists and caller is the nominee.
pub fn claim_ownership(env: &Env, claimer: Address) -> Result<(), ContractError> {
    let pending: PendingOwner = env
        .storage()
        .instance()
        .get(&PENDING_OWNER_KEY)
        .ok_or(ContractError::NoPendingOwner)?;

    if pending.nominee != claimer {
        return Err(ContractError::NotAdmin);
    }
    claimer.require_auth();

    let mut data: ContractData = env
        .storage()
        .instance()
        .get(&DATA_KEY)
        .ok_or(ContractError::NotInitialized)?;

    data.admin = claimer;
    env.storage().instance().set(&DATA_KEY, &data);
    env.storage().instance().remove(&PENDING_OWNER_KEY);
    Ok(())
}

// ─── Issue #493: Two-phase admin key revocation ──────────────────────────────
//
// Prevents instant admin key substitution from a single compromised key.
// An admin key change requires EITHER:
//   (a) A secondary independent verification signature from a registered cosigner, OR
//   (b) A 24-hour timelock period to elapse before the change becomes active.
//
// This gives the network window to detect and respond to a compromised key
// before the damage is done.

/// Phase 1: current admin proposes a new admin key.
/// The change is not active until it passes through one of the two verification paths.
pub fn propose_admin_change(
    env: &Env,
    current_admin: Address,
    new_admin: Address,
) -> Result<(), ContractError> {
    let data: ContractData = env
        .storage()
        .instance()
        .get(&DATA_KEY)
        .ok_or(ContractError::NotInitialized)?;

    if data.admin != current_admin {
        return Err(ContractError::NotAdmin);
    }
    if env.storage().instance().has(&PENDING_ADMIN_KEY) {
        return Err(ContractError::AdminChangePending);
    }
    current_admin.require_auth();

    env.storage().instance().set(
        &PENDING_ADMIN_KEY,
        &AdminChangeProposal {
            new_admin,
            proposer: current_admin,
            proposed_at: env.ledger().timestamp(),
        },
    );
    Ok(())
}

/// Phase 2 — path A: a registered cosigner independently approves the change.
/// Executes the admin key change immediately without waiting for the timelock.
/// The cosigner must be distinct from the proposer.
pub fn countersign_admin_change(
    env: &Env,
    cosigner: Address,
) -> Result<(), ContractError> {
    let proposal: AdminChangeProposal = env
        .storage()
        .instance()
        .get(&PENDING_ADMIN_KEY)
        .ok_or(ContractError::NoAdminChangePending)?;

    if proposal.proposer == cosigner {
        return Err(ContractError::CosignerCannotBeProposer);
    }

    let authorized_signers: Map<Address, ()> = env
        .storage()
        .instance()
        .get(&SIGNERS_KEY)
        .unwrap_or_else(|| Map::new(env));
    let data: ContractData = env
        .storage()
        .instance()
        .get(&DATA_KEY)
        .ok_or(ContractError::NotInitialized)?;

    let is_authorized =
        authorized_signers.contains_key(cosigner.clone()) || data.admin == cosigner;
    if !is_authorized {
        return Err(ContractError::Unauthorized);
    }
    cosigner.require_auth();

    let mut contract_data = data;
    contract_data.admin = proposal.new_admin;
    env.storage().instance().set(&DATA_KEY, &contract_data);
    env.storage().instance().remove(&PENDING_ADMIN_KEY);
    Ok(())
}

/// Phase 2 — path B: execute the admin change after the 24-hour timelock has elapsed.
/// No secondary signature required; the delay itself acts as the verification window.
pub fn execute_admin_change_by_timelock(
    env: &Env,
    executor: Address,
) -> Result<(), ContractError> {
    let proposal: AdminChangeProposal = env
        .storage()
        .instance()
        .get(&PENDING_ADMIN_KEY)
        .ok_or(ContractError::NoAdminChangePending)?;

    let elapsed = env.ledger().timestamp().saturating_sub(proposal.proposed_at);
    if elapsed < ADMIN_CHANGE_TIMELOCK_SECONDS {
        return Err(ContractError::AdminChangeTimelockNotSatisfied);
    }

    let data: ContractData = env
        .storage()
        .instance()
        .get(&DATA_KEY)
        .ok_or(ContractError::NotInitialized)?;

    if executor != proposal.proposer && executor != data.admin {
        return Err(ContractError::Unauthorized);
    }
    executor.require_auth();

    let mut contract_data = data;
    contract_data.admin = proposal.new_admin;
    env.storage().instance().set(&DATA_KEY, &contract_data);
    env.storage().instance().remove(&PENDING_ADMIN_KEY);
    Ok(())
}

/// Cancel a pending admin change. Only the current admin can cancel.
/// Provides an emergency stop if the proposer's key was compromised.
pub fn cancel_admin_change(
    env: &Env,
    canceller: Address,
) -> Result<(), ContractError> {
    let _proposal: AdminChangeProposal = env
        .storage()
        .instance()
        .get(&PENDING_ADMIN_KEY)
        .ok_or(ContractError::NoAdminChangePending)?;

    let data: ContractData = env
        .storage()
        .instance()
        .get(&DATA_KEY)
        .ok_or(ContractError::NotInitialized)?;

    if data.admin != canceller {
        return Err(ContractError::NotAdmin);
    }
    canceller.require_auth();

    env.storage().instance().remove(&PENDING_ADMIN_KEY);
    Ok(())
}

/// Query the currently pending admin change proposal, if any.
pub fn get_pending_admin_change(env: &Env) -> Option<AdminChangeProposal> {
    env.storage().instance().get(&PENDING_ADMIN_KEY)
}
