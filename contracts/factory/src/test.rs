#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Bytes, BytesN, Env, String,
};

mod escrow_wasm {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32v1-none/release/trustescrow_escrow.wasm"
    );
}

const START: u64 = 1_700_000_000;
const DAY: u64 = 86_400;
const AMOUNT: i128 = 500_000_000;
const FEE_BPS: u32 = 150;
const CODE: &[u8] = b"K7M29XQF4TBNR3WD";

struct Setup<'a> {
    env: Env,
    admin: Address,
    arbitrator: Address,
    fee_recipient: Address,
    token: Address,
    factory: FactoryClient<'a>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let escrow_wasm_hash = env.deployer().upload_contract_wasm(escrow_wasm::WASM);

    let id = env.register(
        Factory,
        (Config {
            admin: admin.clone(),
            escrow_wasm_hash,
            arbitrator: arbitrator.clone(),
            fee_recipient: fee_recipient.clone(),
            fee_bps: FEE_BPS,
        },),
    );
    let factory = FactoryClient::new(&env, &id);
    factory.allow_token(&token, &true);

    Setup {
        env,
        admin,
        arbitrator,
        fee_recipient,
        token,
        factory,
    }
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
    fn order(&self, buyer: &Address) -> Order {
        Order {
            buyer: buyer.clone(),
            seller: Address::generate(&self.env),
            token: self.token.clone(),
            amount: AMOUNT,
            terms_hash: BytesN::from_array(&self.env, &[1; 32]),
            release_code_hash: self
                .env
                .crypto()
                .sha256(&Bytes::from_slice(&self.env, CODE))
                .into(),
            funding_deadline: START + DAY,
            delivery_window: 7 * DAY,
            receipt_window: 3 * DAY,
            arbitration_window: 30 * DAY,
        }
    }

    fn salt(&self, n: u8) -> BytesN<32> {
        BytesN::from_array(&self.env, &[n; 32])
    }

    fn escrow(&self, address: &Address) -> escrow_wasm::Client<'_> {
        escrow_wasm::Client::new(&self.env, address)
    }
}

#[test]
fn create_deploys_escrow_at_predicted_address_with_factory_config() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let order = s.order(&buyer);
    let predicted = s.factory.escrow_address(&buyer, &s.salt(1));

    let address = s.factory.create(&order, &s.salt(1));
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, buyer);
    assert_eq!(address, predicted);

    let e = s.escrow(&address).get();
    assert_eq!(e.buyer, buyer);
    assert_eq!(e.seller, order.seller);
    assert_eq!(e.arbitrator, s.arbitrator);
    assert_eq!(e.fee_bps, FEE_BPS);
    assert_eq!(e.fee_recipient, s.fee_recipient);
    assert_eq!(e.release_code_hash, order.release_code_hash);
    assert_eq!(e.state, escrow_wasm::State::Created);
}

#[test]
fn factory_escrow_runs_the_two_sided_flow() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let order = s.order(&buyer);
    StellarAssetClient::new(&s.env, &s.token).mint(&buyer, &AMOUNT);

    let escrow = s.escrow(&s.factory.create(&order, &s.salt(1)));
    escrow.fund();
    escrow.submit_proof(
        &escrow_wasm::ProofKind::Tracking,
        &String::from_str(&s.env, "https://track.example/ABC123"),
        &BytesN::from_array(&s.env, &[7; 32]),
    );
    escrow.release_with_code(&Bytes::from_slice(&s.env, CODE));

    let token = TokenClient::new(&s.env, &s.token);
    let fee = AMOUNT * FEE_BPS as i128 / 10_000;
    assert_eq!(token.balance(&order.seller), AMOUNT - fee);
    assert_eq!(token.balance(&s.fee_recipient), fee);
    assert_eq!(token.balance(&escrow.address), 0);
    assert_eq!(escrow.get().state, escrow_wasm::State::Released);
}

#[test]
fn create_rejects_token_not_on_allowlist() {
    let s = setup();
    let mut order = s.order(&Address::generate(&s.env));
    order.token = s
        .env
        .register_stellar_asset_contract_v2(Address::generate(&s.env))
        .address();
    assert_err(
        s.factory.try_create(&order, &s.salt(1)),
        Error::TokenNotAllowed,
    );
}

#[test]
fn removed_token_can_no_longer_be_used() {
    let s = setup();
    s.factory.allow_token(&s.token, &false);
    assert!(!s.factory.is_token_allowed(&s.token));
    let order = s.order(&Address::generate(&s.env));
    assert_err(
        s.factory.try_create(&order, &s.salt(1)),
        Error::TokenNotAllowed,
    );
}

#[test]
fn invalid_order_is_rejected_by_the_escrow() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let mut order = s.order(&buyer);
    order.seller = buyer;
    assert!(s.factory.try_create(&order, &s.salt(1)).is_err());
}

#[test]
fn salts_are_scoped_to_the_buyer() {
    let s = setup();
    let (a, b) = (Address::generate(&s.env), Address::generate(&s.env));
    let escrow_a = s.factory.create(&s.order(&a), &s.salt(1));
    let escrow_b = s.factory.create(&s.order(&b), &s.salt(1));
    assert_ne!(escrow_a, escrow_b);
    assert!(s.factory.try_create(&s.order(&a), &s.salt(1)).is_err());
}

#[test]
fn config_changes_apply_only_to_new_escrows() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let first = s.factory.create(&s.order(&buyer), &s.salt(1));

    let new_arbitrator = Address::generate(&s.env);
    let mut config = s.factory.config();
    config.arbitrator = new_arbitrator.clone();
    config.fee_bps = 300;
    s.factory.set_config(&config);
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, s.admin);

    let second = s.factory.create(&s.order(&buyer), &s.salt(2));

    let old = s.escrow(&first).get();
    assert_eq!(old.arbitrator, s.arbitrator);
    assert_eq!(old.fee_bps, FEE_BPS);
    let new = s.escrow(&second).get();
    assert_eq!(new.arbitrator, new_arbitrator);
    assert_eq!(new.fee_bps, 300);
}

#[test]
fn fee_above_cap_is_rejected() {
    let s = setup();
    let mut config = s.factory.config();
    config.fee_bps = MAX_FEE_BPS + 1;
    assert_err(s.factory.try_set_config(&config), Error::InvalidFee);
}

#[test]
fn admin_signs_allowlist_changes() {
    let s = setup();
    let token = Address::generate(&s.env);
    s.factory.allow_token(&token, &true);
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, s.admin);
    assert!(s.factory.is_token_allowed(&token));
}
