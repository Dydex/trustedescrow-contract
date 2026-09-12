#![no_std]
//! Types shared by the escrow and factory contracts.
//!
//! Both contracts must agree on the shape of these types. Keeping the
//! definitions in one crate means they cannot drift.

use soroban_sdk::{contracttype, BytesN, String};

/// Upper bound on the platform fee, in basis points (10%).
pub const MAX_FEE_BPS: u32 = 1_000;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Created,
    Funded,
    Delivered,
    Disputed,
    Released,
    Refunded,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProofKind {
    /// Carrier tracking reference — physical goods.
    Tracking,
    /// File or artifact hash — digital goods.
    Content,
    /// Seller statement — services and in-person handovers, weakest tier.
    Attestation,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proof {
    pub kind: ProofKind,
    pub uri: String,
    pub hash: BytesN<32>,
    pub submitted_at: u64,
}
