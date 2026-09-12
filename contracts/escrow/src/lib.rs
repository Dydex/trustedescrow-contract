#![no_std]
//! TrustEscrow escrow contract. One instance per trade, deployed by the factory.
//!
//! The seller is paid only when both sides have spoken — seller proof on-chain
//! plus the buyer's delivery code or signature — or when the arbitrator rules.
//! No timeout pays the seller.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, token,
    Address, Bytes, BytesN, Env, String,
};

pub use trustescrow_types::{
    Dispute, DisputeOrigin, DisputeRecord, Escrow, EscrowParams, Order, Outcome, Proof, ProofKind,
    ProofRecord, RefundPath, ReleasePath, Settlement, State, MAX_FEE_BPS,
};

pub const MAX_URI_LEN: u32 = 256;
pub const MIN_WINDOW: u64 = 60 * 60;
pub const MAX_WINDOW: u64 = 365 * 24 * 60 * 60;

const BPS_DENOMINATOR: i128 = 10_000;
const ALLOWED_SCHEMES: [&[u8]; 3] = [b"https://", b"ipfs://", b"ar://"];

const DAY_IN_LEDGERS: u32 = 17_280;
const TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;
const TTL_EXTEND_TO: u32 = 120 * DAY_IN_LEDGERS;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    InvalidState = 1,
    NotParticipant = 2,
    InvalidCode = 3,
    ProofAlreadySubmitted = 4,
    ProofRequired = 5,
    DeadlinePassed = 6,
    DeadlineNotReached = 7,
    InvalidParties = 8,
    InvalidAmount = 9,
    InvalidFee = 10,
    InvalidWindow = 11,
    InvalidUri = 12,
    Overflow = 13,
}

#[contracttype]
enum DataKey {
    Escrow,
}

#[contractevent(topics = ["created"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Created {
    #[topic]
    pub buyer: Address,
    #[topic]
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
}

#[contractevent(topics = ["funded"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Funded {
    pub amount: i128,
    pub delivery_deadline: u64,
}

#[contractevent(topics = ["proof"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofSubmitted {
    pub kind: ProofKind,
    pub hash: BytesN<32>,
    pub receipt_deadline: u64,
}

#[contractevent(topics = ["released"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Released {
    pub path: ReleasePath,
    pub payout: i128,
    pub fee: i128,
}

#[contractevent(topics = ["cancelled"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cancelled {
    pub by: Address,
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    pub fn __constructor(env: Env, params: EscrowParams) {
        let now = now(&env);
        let order = params.order;

        if order.buyer == order.seller
            || params.arbitrator == order.buyer
            || params.arbitrator == order.seller
        {
            panic_with_error!(&env, Error::InvalidParties);
        }
        if order.amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if params.fee_bps > MAX_FEE_BPS {
            panic_with_error!(&env, Error::InvalidFee);
        }
        if order.funding_deadline <= now || order.funding_deadline > add(&env, now, MAX_WINDOW) {
            panic_with_error!(&env, Error::InvalidWindow);
        }
        for window in [
            order.delivery_window,
            order.receipt_window,
            order.arbitration_window,
        ] {
            if !(MIN_WINDOW..=MAX_WINDOW).contains(&window) {
                panic_with_error!(&env, Error::InvalidWindow);
            }
        }

        let escrow = Escrow {
            buyer: order.buyer,
            seller: order.seller,
            arbitrator: params.arbitrator,
            token: order.token,
            amount: order.amount,
            fee_bps: params.fee_bps,
            fee_recipient: params.fee_recipient,
            terms_hash: order.terms_hash,
            release_code_hash: order.release_code_hash,
            state: State::Created,
            created_at: now,
            funding_deadline: order.funding_deadline,
            delivery_window: order.delivery_window,
            receipt_window: order.receipt_window,
            arbitration_window: order.arbitration_window,
            funded_at: 0,
            delivery_deadline: 0,
            receipt_deadline: 0,
            proof: ProofRecord::Pending,
            dispute: DisputeRecord::NotOpened,
            settlement: Settlement::Open,
        };
        save(&env, &escrow);

        Created {
            buyer: escrow.buyer,
            seller: escrow.seller,
            token: escrow.token,
            amount: escrow.amount,
        }
        .publish(&env);
    }

    /// Buyer deposits `amount`. Starts the delivery window.
    pub fn fund(env: Env) {
        let mut e = load(&env);
        require_state(&env, &e, State::Created);
        let now = now(&env);
        if now >= e.funding_deadline {
            panic_with_error!(&env, Error::DeadlinePassed);
        }
        e.buyer.require_auth();

        e.state = State::Funded;
        e.funded_at = now;
        e.delivery_deadline = add(&env, now, e.delivery_window);
        save(&env, &e);

        token::TokenClient::new(&env, &e.token).transfer(
            &e.buyer,
            env.current_contract_address(),
            &e.amount,
        );
        Funded {
            amount: e.amount,
            delivery_deadline: e.delivery_deadline,
        }
        .publish(&env);
    }

    /// Abandon an unfunded escrow. Either party may cancel before
    /// `funding_deadline`; anyone may after it. No funds are held.
    pub fn cancel(env: Env, caller: Address) {
        caller.require_auth();
        let mut e = load(&env);
        require_state(&env, &e, State::Created);
        let is_party = caller == e.buyer || caller == e.seller;
        if !is_party && now(&env) < e.funding_deadline {
            panic_with_error!(&env, Error::NotParticipant);
        }

        e.state = State::Cancelled;
        save(&env, &e);
        Cancelled { by: caller }.publish(&env);
    }

    /// Seller commits proof of delivery. Single-shot. Starts the buyer's
    /// receipt window; it does not by itself entitle the seller to anything.
    pub fn submit_proof(env: Env, kind: ProofKind, uri: String, hash: BytesN<32>) {
        let mut e = load(&env);
        if e.proof().is_some() {
            panic_with_error!(&env, Error::ProofAlreadySubmitted);
        }
        require_state(&env, &e, State::Funded);
        e.seller.require_auth();
        let now = now(&env);
        if now >= e.delivery_deadline {
            panic_with_error!(&env, Error::DeadlinePassed);
        }
        validate_uri(&env, kind, &uri);

        e.receipt_deadline = add(&env, now, e.receipt_window);
        e.proof = ProofRecord::Submitted(Proof {
            kind,
            uri,
            hash: hash.clone(),
            submitted_at: now,
        });
        e.state = State::Delivered;
        save(&env, &e);

        ProofSubmitted {
            kind,
            hash,
            receipt_deadline: e.receipt_deadline,
        }
        .publish(&env);
    }

    /// In-person handover: the seller records proof and presents the buyer's
    /// code in one transaction. Accepted after `delivery_deadline` too — a
    /// buyer who hands over the code has accepted late delivery.
    pub fn submit_proof_with_code(
        env: Env,
        kind: ProofKind,
        uri: String,
        hash: BytesN<32>,
        code: Bytes,
    ) {
        let mut e = load(&env);
        if e.proof().is_some() {
            panic_with_error!(&env, Error::ProofAlreadySubmitted);
        }
        require_state(&env, &e, State::Funded);
        e.seller.require_auth();
        validate_uri(&env, kind, &uri);
        verify_code(&env, &e, &code);

        let now = now(&env);
        e.receipt_deadline = now;
        e.proof = ProofRecord::Submitted(Proof {
            kind,
            uri,
            hash: hash.clone(),
            submitted_at: now,
        });
        ProofSubmitted {
            kind,
            hash,
            receipt_deadline: now,
        }
        .publish(&env);
        release(&env, e, ReleasePath::Code);
    }

    /// Release on the buyer's delivery code. Callable by anyone holding it, but
    /// only once the seller's proof is on-chain.
    pub fn release_with_code(env: Env, code: Bytes) {
        let e = load(&env);
        match e.state {
            State::Delivered => {}
            State::Funded => panic_with_error!(&env, Error::ProofRequired),
            _ => panic_with_error!(&env, Error::InvalidState),
        }
        verify_code(&env, &e, &code);
        release(&env, e, ReleasePath::Code);
    }

    /// Buyer confirms receipt with their own signature — the fallback for a
    /// buyer who has lost their code.
    pub fn confirm(env: Env) {
        let e = load(&env);
        match e.state {
            State::Delivered => {}
            State::Funded => panic_with_error!(&env, Error::ProofRequired),
            _ => panic_with_error!(&env, Error::InvalidState),
        }
        e.buyer.require_auth();
        release(&env, e, ReleasePath::Confirmation);
    }

    pub fn get(env: Env) -> Escrow {
        load(&env)
    }

    /// Extend the instance TTL. Public so a bumper job can keep idle escrows live.
    pub fn bump(env: Env) {
        extend_ttl(&env);
    }
}

fn now(env: &Env) -> u64 {
    env.ledger().timestamp()
}

fn add(env: &Env, a: u64, b: u64) -> u64 {
    a.checked_add(b)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow))
}

fn load(env: &Env) -> Escrow {
    env.storage().instance().get(&DataKey::Escrow).unwrap()
}

fn save(env: &Env, e: &Escrow) {
    env.storage().instance().set(&DataKey::Escrow, e);
    extend_ttl(env);
}

fn extend_ttl(env: &Env) {
    let extend_to = TTL_EXTEND_TO.min(env.storage().max_ttl());
    let threshold = TTL_THRESHOLD.min(extend_to);
    env.storage().instance().extend_ttl(threshold, extend_to);
}

fn require_state(env: &Env, e: &Escrow, state: State) {
    if e.state != state {
        panic_with_error!(env, Error::InvalidState);
    }
}

fn verify_code(env: &Env, e: &Escrow, code: &Bytes) {
    let hash: BytesN<32> = env.crypto().sha256(code).into();
    if hash != e.release_code_hash {
        panic_with_error!(env, Error::InvalidCode);
    }
}

fn validate_uri(env: &Env, kind: ProofKind, uri: &String) {
    let len = uri.len();
    if len == 0 {
        if kind == ProofKind::Attestation {
            return;
        }
        panic_with_error!(env, Error::InvalidUri);
    }
    if len > MAX_URI_LEN {
        panic_with_error!(env, Error::InvalidUri);
    }
    let mut buf = [0u8; MAX_URI_LEN as usize];
    let bytes = &mut buf[..len as usize];
    uri.copy_into_slice(bytes);

    let printable = bytes.iter().all(|b| (0x21..=0x7e).contains(b));
    let has_scheme = ALLOWED_SCHEMES
        .iter()
        .any(|scheme| bytes.len() > scheme.len() && bytes.starts_with(scheme));
    if !printable || !has_scheme {
        panic_with_error!(env, Error::InvalidUri);
    }
}

fn release(env: &Env, mut e: Escrow, path: ReleasePath) {
    let fee = e
        .amount
        .checked_mul(e.fee_bps as i128)
        .unwrap_or_else(|| panic_with_error!(env, Error::Overflow))
        / BPS_DENOMINATOR;
    let payout = e.amount - fee;

    e.state = State::Released;
    e.settlement = Settlement::Released(path);
    save(env, &e);

    let token = token::TokenClient::new(env, &e.token);
    let this = env.current_contract_address();
    token.transfer(&this, &e.seller, &payout);
    if fee > 0 {
        token.transfer(&this, &e.fee_recipient, &fee);
    }
    Released { path, payout, fee }.publish(env);
}

#[cfg(test)]
mod test;
