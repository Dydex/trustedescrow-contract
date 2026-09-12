# TrustEscrow contracts

Soroban contracts for TrustEscrow, a peer-to-peer escrow for online trade between strangers.

A buyer deposits, the seller proves delivery on-chain, the buyer proves receipt (by handing over a delivery code or signing a confirmation), and only then is the seller paid. No timeout ever pays the seller: a silent buyer escalates to an arbitrator, and an arbitrator who doesn't rule in time refunds the buyer.

## Layout

| Path | Crate | What it is |
|---|---|---|
| [crates/types](crates/types) | `trustescrow-types` | Types shared by both contracts |
| [contracts/escrow](contracts/escrow) | `trustescrow-escrow` | One instance per trade: custody, lifecycle, payout |
| [contracts/factory](contracts/factory) | `trustescrow-factory` | Deploys escrows; token allowlist and operator config for new escrows |

## Build and test

Requires Rust stable with the `wasm32v1-none` target (`rust-toolchain.toml` pins both).

```sh
make build   # release WASM into target/wasm32v1-none/release/
make test    # builds the WASM first — the factory tests deploy the real escrow binary
make clippy
```

`cargo test` on its own fails to compile the factory tests if the escrow WASM hasn't been built yet.

## Lifecycle at a glance

```
Created ──fund──▶ Funded ──submit_proof──▶ Delivered ──release_with_code / confirm──▶ Released
   │                │  └─submit_proof_with_code (in person)─────────────────────────▶ Released
   │                │                         │
   ▼                ▼                         ▼ dispute / escalate (after receipt_deadline)
Cancelled        Refunded ◀── delivery      Disputed ──resolve──▶ Released | Refunded
                 timeout / seller_refund      └── arbitration deadline ──▶ Refunded
```

## Deploying (testnet)

With the [Stellar CLI](https://developers.stellar.org/docs/tools/cli):

```sh
stellar contract upload --wasm target/wasm32v1-none/release/trustescrow_escrow.wasm --network testnet --source <admin>
# → escrow WASM hash

stellar contract deploy --wasm target/wasm32v1-none/release/trustescrow_factory.wasm --network testnet --source <admin> \
  -- --config '{"admin":"<admin>","escrow_wasm_hash":"<hash>","arbitrator":"<arbitrator>","fee_recipient":"<fee>","fee_bps":150}'

stellar contract invoke --id <factory> --network testnet --source <admin> -- allow_token --token <usdc-sac> --allowed true
```

Clients must pin the escrow WASM hash they have audited and refuse to fund an escrow instance running anything else. A factory config change only affects escrows created after it, so a swapped WASM hash can never reach an open trade.

## Status

v1 draft, not audited. Do not use on mainnet without an external review.
