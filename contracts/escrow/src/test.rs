#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Bytes, BytesN, Env, String,
};

const AMOUNT: i128 = 1_000_000_000;
const FEE_BPS: u32 = 150;
const START: u64 = 1_700_000_000;
const DAY: u64 = 86_400;
const DELIVERY_WINDOW: u64 = 7 * DAY;
const RECEIPT_WINDOW: u64 = 3 * DAY;
const ARBITRATION_WINDOW: u64 = 30 * DAY;
const CODE: &[u8] = b"K7M29XQF4TBNR3WD";
const TRACKING_URI: &str = "https://track.example/ABC123";

struct Setup<'a> {
    env: Env,
    buyer: Address,
    seller: Address,
    arbitrator: Address,
    fee_recipient: Address,
    token: TokenClient<'a>,
    escrow: EscrowContractClient<'a>,
}

fn new_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    env
}

fn default_params(env: &Env) -> EscrowParams {
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    EscrowParams {
        order: Order {
            buyer: Address::generate(env),
            seller: Address::generate(env),
            token,
            amount: AMOUNT,
            terms_hash: BytesN::from_array(env, &[1; 32]),
            release_code_hash: env.crypto().sha256(&Bytes::from_slice(env, CODE)).into(),
            funding_deadline: START + DAY,
            delivery_window: DELIVERY_WINDOW,
            receipt_window: RECEIPT_WINDOW,
            arbitration_window: ARBITRATION_WINDOW,
        },
        arbitrator: Address::generate(env),
        fee_bps: FEE_BPS,
        fee_recipient: Address::generate(env),
    }
}

fn setup_from<'a>(env: Env, params: EscrowParams) -> Setup<'a> {
    let order = &params.order;
    StellarAssetClient::new(&env, &order.token).mint(&order.buyer, &order.amount);
    let id = env.register(EscrowContract, (params.clone(),));
    Setup {
        token: TokenClient::new(&env, &order.token),
        escrow: EscrowContractClient::new(&env, &id),
        buyer: order.buyer.clone(),
        seller: order.seller.clone(),
        arbitrator: params.arbitrator.clone(),
        fee_recipient: params.fee_recipient.clone(),
        env,
    }
}

fn setup<'a>() -> Setup<'a> {
    let env = new_env();
    let params = default_params(&env);
    setup_from(env, params)
}

/// Entry points panic with a contract error rather than returning `Result`, so
/// the generated `try_` client surfaces it as a generic `soroban_sdk::Error`.
fn assert_err<T: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<T, Result<soroban_sdk::Error, E>>,
    expected: Error,
) {
    match result {
        Err(Ok(got)) => assert_eq!(got, soroban_sdk::Error::from(expected)),
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

impl Setup<'_> {
    fn at(&self, timestamp: u64) {
        self.env.ledger().set_timestamp(timestamp);
    }

    fn get(&self) -> Escrow {
        self.escrow.get()
    }

    fn state(&self) -> State {
        self.get().state
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token.balance(who)
    }

    fn uri(&self, s: &str) -> String {
        String::from_str(&self.env, s)
    }

    fn hash(&self) -> BytesN<32> {
        BytesN::from_array(&self.env, &[7; 32])
    }

    fn deliver(&self) {
        self.escrow
            .submit_proof(&ProofKind::Tracking, &self.uri(TRACKING_URI), &self.hash());
    }

    fn funded(self) -> Self {
        self.escrow.fund();
        self
    }

    fn delivered(self) -> Self {
        self.escrow.fund();
        self.deliver();
        self
    }

    fn assert_only_auth(&self, who: &Address) {
        let auths = self.env.auths();
        assert_eq!(auths.len(), 1, "expected exactly one signer, got {auths:?}");
        assert_eq!(&auths[0].0, who);
    }
}

// --- Construction ------------------------------------------------------------

#[test]
fn constructor_records_order_and_operator_config() {
    let s = setup();
    let e = s.get();
    assert_eq!(e.buyer, s.buyer);
    assert_eq!(e.seller, s.seller);
    assert_eq!(e.arbitrator, s.arbitrator);
    assert_eq!(e.fee_recipient, s.fee_recipient);
    assert_eq!(e.fee_bps, FEE_BPS);
    assert_eq!(e.amount, AMOUNT);
    assert_eq!(e.state, State::Created);
    assert_eq!(e.created_at, START);
    assert_eq!(e.funding_deadline, START + DAY);
    assert_eq!(e.delivery_window, DELIVERY_WINDOW);
    assert_eq!(e.receipt_window, RECEIPT_WINDOW);
    assert_eq!(e.arbitration_window, ARBITRATION_WINDOW);
    assert_eq!(
        (e.funded_at, e.delivery_deadline, e.receipt_deadline),
        (0, 0, 0)
    );
    assert!(e.proof().is_none());
    assert!(e.dispute().is_none());
    assert_eq!(e.settlement, Settlement::Open);
    assert_eq!(s.balance(&s.escrow.address), 0);
    assert_eq!(s.balance(&s.buyer), AMOUNT);
}

#[test]
fn bump_is_permissionless() {
    let s = setup();
    s.escrow.bump();
    assert!(s.env.auths().is_empty());
}

fn register_with(mutate: impl FnOnce(&mut EscrowParams)) {
    let env = new_env();
    let mut params = default_params(&env);
    mutate(&mut params);
    env.register(EscrowContract, (params,));
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn buyer_cannot_be_seller() {
    register_with(|p| p.order.seller = p.order.buyer.clone());
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn arbitrator_cannot_be_a_party() {
    register_with(|p| p.arbitrator = p.order.seller.clone());
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn amount_must_be_positive() {
    register_with(|p| p.order.amount = 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn fee_is_capped() {
    register_with(|p| p.fee_bps = MAX_FEE_BPS + 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn windows_have_a_minimum() {
    register_with(|p| p.order.receipt_window = MIN_WINDOW - 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn funding_deadline_must_be_in_the_future() {
    register_with(|p| p.order.funding_deadline = START);
}

// --- Funding and cancellation ------------------------------------------------

#[test]
fn fund_moves_tokens_and_starts_delivery_window() {
    let s = setup();
    s.escrow.fund();
    s.assert_only_auth(&s.buyer);
    let e = s.get();
    assert_eq!(e.state, State::Funded);
    assert_eq!(e.funded_at, START);
    assert_eq!(e.delivery_deadline, START + DELIVERY_WINDOW);
    assert_eq!(s.balance(&s.buyer), 0);
    assert_eq!(s.balance(&s.escrow.address), AMOUNT);
}

#[test]
fn fund_twice_is_rejected() {
    let s = setup().funded();
    assert_err(s.escrow.try_fund(), Error::InvalidState);
}

#[test]
fn fund_is_rejected_at_funding_deadline() {
    let s = setup();
    s.at(START + DAY);
    assert_err(s.escrow.try_fund(), Error::DeadlinePassed);
}

#[test]
fn either_party_can_cancel_before_funding() {
    for party in [0, 1] {
        let s = setup();
        let who = if party == 0 {
            s.buyer.clone()
        } else {
            s.seller.clone()
        };
        s.escrow.cancel(&who);
        assert_eq!(s.state(), State::Cancelled);
    }
}

#[test]
fn stranger_can_cancel_only_after_funding_deadline() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.at(START + DAY - 1);
    assert_err(s.escrow.try_cancel(&stranger), Error::NotParticipant);
    s.at(START + DAY);
    s.escrow.cancel(&stranger);
    assert_eq!(s.state(), State::Cancelled);
}

#[test]
fn funded_escrow_cannot_be_cancelled() {
    let s = setup().funded();
    assert_err(s.escrow.try_cancel(&s.buyer), Error::InvalidState);
}

// --- Proof -------------------------------------------------------------------

#[test]
fn seller_signs_proof_and_starts_receipt_window() {
    let s = setup().funded();
    s.at(START + DAY);
    s.deliver();
    s.assert_only_auth(&s.seller);
    let e = s.get();
    assert_eq!(e.state, State::Delivered);
    assert_eq!(e.receipt_deadline, START + DAY + RECEIPT_WINDOW);
    let proof = e.proof().unwrap();
    assert_eq!(proof.submitted_at, START + DAY);
    assert_eq!(proof.uri, s.uri(TRACKING_URI));
}

#[test]
fn proof_requires_a_funded_escrow() {
    let s = setup();
    assert_err(
        s.escrow
            .try_submit_proof(&ProofKind::Tracking, &s.uri(TRACKING_URI), &s.hash()),
        Error::InvalidState,
    );
}

#[test]
fn proof_is_rejected_at_delivery_deadline() {
    let s = setup().funded();
    s.at(s.get().delivery_deadline);
    assert_err(
        s.escrow
            .try_submit_proof(&ProofKind::Tracking, &s.uri(TRACKING_URI), &s.hash()),
        Error::DeadlinePassed,
    );
    assert_eq!(s.state(), State::Funded);
}

#[test]
fn proof_is_single_shot() {
    let s = setup().delivered();
    let original = s.get().proof;
    assert_err(
        s.escrow.try_submit_proof(
            &ProofKind::Content,
            &s.uri("ipfs://other"),
            &BytesN::from_array(&s.env, &[8; 32]),
        ),
        Error::ProofAlreadySubmitted,
    );
    assert_eq!(s.get().proof, original);
}

#[test]
fn invalid_proof_uris_are_rejected() {
    let s = setup().funded();
    let too_long = std::format!("https://{}", "a".repeat(249));
    assert_eq!(too_long.len(), 257);
    let cases = [
        (ProofKind::Tracking, ""),
        (ProofKind::Content, ""),
        (ProofKind::Tracking, "http://track.example/1"),
        (ProofKind::Tracking, "ftp://track.example/1"),
        (ProofKind::Tracking, "https://"),
        (ProofKind::Tracking, "https://track.example/a b"),
        (ProofKind::Tracking, too_long.as_str()),
    ];
    for (kind, uri) in cases {
        assert_err(
            s.escrow.try_submit_proof(&kind, &s.uri(uri), &s.hash()),
            Error::InvalidUri,
        );
    }
    assert_eq!(s.state(), State::Funded);
}

#[test]
fn valid_proof_uris_are_accepted() {
    let max_len = std::format!("https://{}", "a".repeat(248));
    assert_eq!(max_len.len(), 256);
    let cases = [
        (ProofKind::Attestation, ""),
        (
            ProofKind::Content,
            "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
        ),
        (
            ProofKind::Content,
            "ar://bNbA3TEQVL60xlgCcqdz4ZPHFZ711cZ3hmkpGttDt_U",
        ),
        (ProofKind::Tracking, max_len.as_str()),
    ];
    for (kind, uri) in cases {
        let s = setup().funded();
        s.escrow.submit_proof(&kind, &s.uri(uri), &s.hash());
        assert_eq!(s.get().proof().unwrap().uri, s.uri(uri));
    }
}
