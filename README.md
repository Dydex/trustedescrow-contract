# TrustEscrow contracts

Soroban contracts for TrustEscrow, a peer-to-peer escrow for online trade between strangers.

A buyer deposits, the seller proves delivery on-chain, the buyer proves receipt (by handing over a delivery code or signing a confirmation), and only then is the seller paid. No timeout ever pays the seller: a silent buyer escalates to an arbitrator, and an arbitrator who doesn't rule in time refunds the buyer.

## Layout

| Path | Crate | What it is |
|---|---|---|
| [crates/types](crates/types) | `trustescrow-types` | Types shared by both contracts |
| [crates/code](crates/code) | `trustescrow-code` | Reference implementation of the delivery code: encoding, normalisation, hash |
| [contracts/escrow](contracts/escrow) | `trustescrow-escrow` | One instance per trade: custody, lifecycle, payout |
| [contracts/factory](contracts/factory) | `trustescrow-factory` | Deploys escrows; token allowlist, operator config for new escrows, two-step admin transfer |
| [test-vectors](test-vectors) | — | Delivery code vectors every client must pass |
| [scripts](scripts) | — | Testnet deployment |

## Build and test

Requires Rust stable with the `wasm32v1-none` target (`rust-toolchain.toml` pins both).

```sh
make build   # release WASM into target/wasm32v1-none/release/
make test    # builds the WASM first — the factory tests deploy the real escrow binary
make clippy
```

`cargo test` on its own fails to compile the factory tests if the escrow WASM hasn't been built yet. Dependencies are compiled with optimisation even in test builds, because the Soroban host is very slow without it; the first build takes a while, later ones are quick.

Besides unit tests for every transition and deadline boundary, the escrow has a seeded randomised state-machine test that drives escrows through random call sequences and checks conservation, terminality and the two-sided release rule after every step. CI runs formatting, the WASM build, clippy and all tests on every push.

## Lifecycle at a glance

```
Created ──fund──▶ Funded ──submit_proof──▶ Delivered ──release_with_code / confirm──▶ Released
   │                │  └─submit_proof_with_code (in person)─────────────────────────▶ Released
   │                │                         │
   ▼                ▼                         ▼ dispute / escalate (after receipt_deadline)
Cancelled        Refunded ◀── delivery      Disputed ──resolve──▶ Released | Refunded
                 timeout / seller_refund      └── arbitration deadline ──▶ Refunded
```

## Delivery codes

A delivery code is 80 bits of entropy written as 16 Crockford base32 characters and shown as `K7M2-9XQF-4TBN-R3WD`. The escrow stores `sha256` of the 16 canonical characters. Because that hash is public, the code's length is its only defence against brute force: never generate shorter codes.

Clients must normalise input before hashing or submitting it: strip whitespace and hyphens, uppercase, and map `I`/`L` to `1` and `O` to `0`. [crates/code](crates/code) implements this, and [test-vectors/delivery-codes.json](test-vectors/delivery-codes.json) holds vectors produced by an independent implementation. A client that passes them produces the same bytes the contract checks.

## Deploying (testnet)

With the [Stellar CLI](https://developers.stellar.org/docs/tools/cli) and a funded identity:

```sh
SOURCE=admin ARBITRATOR=G... FEE_RECIPIENT=G... TOKEN=C... scripts/deploy-testnet.sh
```

The script uploads the escrow WASM, deploys the factory, allowlists the settlement token and writes the factory id and escrow WASM hash to `deployments/testnet.env`.

Clients must pin the escrow WASM hash they have audited and refuse to fund an escrow instance running anything else. A factory config change only affects escrows created after it, so a swapped WASM hash can never reach an open trade.

Handing the factory to a new admin takes two steps: the current admin calls `propose_admin`, and nothing changes until the proposed address calls `accept_admin`. `set_config` cannot change the admin, so a mistyped address can never lock the factory.


## Live on testnet

| What | Id |
|---|---|
| Factory | `CDMCI4VW5XARBPITNHNENEDVKBMKDICFCKJJ3RYDAKDBZRQMGPUV5JIO` |
| Escrow WASM hash | `7a91c255c29edb7114a546026e807f144a8adc58460e4608e314a77ac629281d` |
| Settlement token (test asset SAC) | `CBXMP6YK4B4WZKN4UAF7OZUEGFEUURVPSUQS5QGG5SG5DRWBRQDWAOOL` |

`deployments/testnet.env` holds the same values plus the admin, arbitrator and fee recipient.

One escrow has been run end to end against this deployment: [create](https://stellar.expert/explorer/testnet/tx/16515feb9764f9ef3021bdbd20744c01d467ce6eee22a8cb31d038b964589e7d), [fund](https://stellar.expert/explorer/testnet/tx/8e305e4323098a99564e985e3ca671e805219ae66250d1e3b17d42a5e15c1c02), [submit_proof](https://stellar.expert/explorer/testnet/tx/48c038bed1a5c6d8428339902cd74d54077cc7c494a51ba3499658db7e1b875a), then release with the buyer's delivery code. Of 100 units deposited the seller received 98.5 and the fee recipient 1.5, and the escrow ended `Released` via `Code` holding nothing.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
