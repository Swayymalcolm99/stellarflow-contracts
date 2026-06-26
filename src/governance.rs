use soroban_sdk::{contracttype, Address, BytesN, Env, Map, Symbol};
use crate::ContractError;

const BALLOT_TTL_LEDGERS: u32 = 17_280;
const BALLOT_TTL_THRESHOLD: u32 = 5_000;

#[contracttype]
#[derive(Clone)]
pub struct StagedUpgrade {
    pub new_wasm_hash: BytesN<32>,
    pub proposer: Address,
    pub staged_at: u64,
}

pub fn verify_staged_delay(staged_at: u64, current_time: u64, delay_seconds: u64) -> bool {
    current_time.saturating_sub(staged_at) >= delay_seconds
}

#[contracttype]
pub enum BallotKey {
    Proposal(Symbol),
}

#[contracttype]
#[derive(Clone)]
pub struct VotingBallot {
    pub target: Address,
    pub replacement: Address,
    pub proposer: Address,
    pub proposed_at: u64,
    pub votes: Map<Address, ()>,
}

pub fn open_ballot(
    env: &Env,
    proposal_id: Symbol,
    target: Address,
    replacement: Address,
    proposer: Address,
) -> Result<(), ContractError> {
    let key = BallotKey::Proposal(proposal_id);
    if env.storage().temporary().has(&key) {
        return Err(ContractError::ProposalAlreadyActive);
    }
    let ballot = VotingBallot {
        target,
        replacement,
        proposer,
        proposed_at: env.ledger().timestamp(),
        votes: Map::new(env),
    };
    env.storage().temporary().set(&key, &ballot);
    env.storage().temporary().extend_ttl(&key, BALLOT_TTL_THRESHOLD, BALLOT_TTL_LEDGERS);
    Ok(())
}

pub fn cast_vote(
    env: &Env,
    proposal_id: Symbol,
    voter: Address,
) -> Result<VotingBallot, ContractError> {
    let key = BallotKey::Proposal(proposal_id);
    let mut ballot: VotingBallot = env
        .storage()
        .temporary()
        .get(&key)
        .ok_or(ContractError::NoActiveProposal)?;
    if ballot.votes.contains_key(voter.clone()) {
        return Err(ContractError::AlreadyVoted);
    }
    ballot.votes.set(voter, ());
    env.storage().temporary().set(&key, &ballot);
    env.storage().temporary().extend_ttl(&key, BALLOT_TTL_THRESHOLD, BALLOT_TTL_LEDGERS);
    Ok(ballot)
}

pub fn get_ballot(env: &Env, proposal_id: Symbol) -> Option<VotingBallot> {
    env.storage().temporary().get(&BallotKey::Proposal(proposal_id))
}

pub fn close_ballot(env: &Env, proposal_id: Symbol) {
    env.storage().temporary().remove(&BallotKey::Proposal(proposal_id));
}
