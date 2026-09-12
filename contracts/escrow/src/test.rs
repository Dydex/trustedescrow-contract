#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Bytes, BytesN, Env,
};

const AMOUNT: i128 = 1_000_000_000;
const FEE_BPS: u32 = 150;
const START: u64 = 1_700_000_000;
const DAY: u64 = 86_400;
const DELIVERY_WINDOW: u64 = 7 * DAY;
const RECEIPT_WINDOW: u64 = 3 * DAY;
const ARBITRATION_WINDOW: u64 = 30 * DAY;
const CODE: &[u8] = b"K7M29XQF4TBNR3WD";

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

impl Setup<'_> {
    fn get(&self) -> Escrow {
        self.escrow.get()
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token.balance(who)
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
    assert_eq!((e.funded_at, e.delivery_deadline, e.receipt_deadline), (0, 0, 0));
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
