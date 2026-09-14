//! Client-side proof-of-work for unverified one-to-one connection starts.
//!
//! Hosts should run `solve` on a background worker. The default difficulty is
//! bounded by the protocol so a server cannot turn this into an unbounded CPU
//! request.

use links_protocol::proof_of_work::{self, ProofOfWorkError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofOfWorkChallenge {
    challenge: [u8; proof_of_work::CHALLENGE_BYTES],
    difficulty_bits: u8,
    expires_at_ms: u64,
}

impl ProofOfWorkChallenge {
    pub fn new(
        challenge: [u8; proof_of_work::CHALLENGE_BYTES],
        difficulty_bits: u8,
        expires_at_ms: u64,
    ) -> Result<Self, ProofOfWorkError> {
        if expires_at_ms == 0 {
            return Err(ProofOfWorkError::InvalidChallenge);
        }
        proof_of_work::validate_difficulty(difficulty_bits)?;
        Ok(Self {
            challenge,
            difficulty_bits,
            expires_at_ms,
        })
    }

    pub fn challenge(&self) -> &[u8; proof_of_work::CHALLENGE_BYTES] {
        &self.challenge
    }

    pub fn difficulty_bits(&self) -> u8 {
        self.difficulty_bits
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProofOfWorkSolution {
    challenge: [u8; proof_of_work::CHALLENGE_BYTES],
    nonce: u64,
}

impl ProofOfWorkSolution {
    pub fn challenge(&self) -> &[u8; proof_of_work::CHALLENGE_BYTES] {
        &self.challenge
    }

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn verify(&self, challenge: &ProofOfWorkChallenge) -> Result<(), ProofOfWorkError> {
        if self.challenge != challenge.challenge {
            return Err(ProofOfWorkError::InvalidChallenge);
        }
        proof_of_work::verify(&self.challenge, challenge.difficulty_bits, self.nonce)
    }
}

/// Search for a valid nonce. Run this on a background worker, never on a UI
/// thread. `max_attempts` bounds local CPU work and also makes cancellation
/// straightforward for mobile hosts.
pub fn solve(
    challenge: &ProofOfWorkChallenge,
    max_attempts: u64,
) -> Result<ProofOfWorkSolution, ProofOfWorkError> {
    for nonce in 0..max_attempts {
        if proof_of_work::verify(&challenge.challenge, challenge.difficulty_bits, nonce).is_ok() {
            return Ok(ProofOfWorkSolution {
                challenge: challenge.challenge,
                nonce,
            });
        }
    }
    Err(ProofOfWorkError::Exhausted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solution_is_bound_to_challenge() {
        let challenge =
            ProofOfWorkChallenge::new([7u8; proof_of_work::CHALLENGE_BYTES], 12, 10_000).unwrap();
        let solution = solve(&challenge, 2_000_000).unwrap();
        assert!(solution.verify(&challenge).is_ok());
        let other =
            ProofOfWorkChallenge::new([8u8; proof_of_work::CHALLENGE_BYTES], 12, 10_000).unwrap();
        assert_eq!(
            solution.verify(&other),
            Err(ProofOfWorkError::InvalidChallenge)
        );
    }

    #[test]
    fn search_budget_is_enforced() {
        let challenge =
            ProofOfWorkChallenge::new([7u8; proof_of_work::CHALLENGE_BYTES], 24, 10_000).unwrap();
        assert_eq!(solve(&challenge, 0), Err(ProofOfWorkError::Exhausted));
    }
}
