# Security

These contracts hold other people's money. Please report problems privately.

## Reporting a vulnerability

Use GitHub's **private vulnerability reporting** (Security tab → *Report a vulnerability*) on this repository. Do not open a public issue or pull request for anything that could put funds at risk.

Include the affected contract and function, the sequence of calls that triggers the problem, and what you expected to happen instead. A failing test is the most useful report there is.

## Status

v1 draft. **Not audited.** Do not deploy to mainnet or hold real value in these contracts until an external review has been completed and published.

## What counts as a vulnerability

Anything that breaks one of these properties:

1. **Conservation.** On release, `payout + fee == amount`, and the seller is paid in full regardless of whether the fee transfer to `fee_recipient` succeeds. On refund, the buyer receives exactly `amount`. A terminal escrow holds no tokens, except a fee that failed to reach `fee_recipient` on release — recoverable at any time via the permissionless `sweep_fee`, and never payable to anyone else.
2. **Destination.** Principal only ever goes to the buyer or the seller; the fee only to `fee_recipient`, and only on release.
3. **Two-sided release.** The seller is paid only with seller proof on-chain plus the buyer's delivery code or signature, or by the arbitrator's ruling. No timeout pays the seller.
4. **Arbitrator bounds.** `resolve` works only in `Disputed`, only for the arbitrator, only before the arbitration deadline, and only chooses release or refund.
5. **No stuck funds.** Every open state has a permissionless exit.
6. **Immutability.** Proof, terms hash and delivery-code hash cannot change once written. Factory config changes never reach existing escrows.
7. **Terminality.** `Released`, `Refunded` and `Cancelled` accept no further transitions.

## Known limitations (not vulnerabilities)

- The arbitrator can decide a dispute in bad faith. It cannot steal or act after its deadline.
- If the arbitrator never rules, the buyer is refunded, even if they received the goods and withheld the code. The seller carries this risk by design.
- A seller or courier can demand the delivery code before handing over the goods. No contract can prevent this; the client must warn the buyer.
- The delivery code's security rests on its 80 bits of entropy. A client that generates shorter codes makes them brute-forceable from the public hash.
- The escrow trusts its token contract. The factory's allowlist is the defence against hostile tokens.
