# Arras

**Non-custodial escrow on NEAR for paying people, AI agents and robots.** A payer locks USDC in the contract with
one `ft_transfer_call`; an operator (Execution Market) releases it to the worker when the job is approved, or it goes
back to the payer by refund or, after a deadline, by reclaim. Version 1 holds a single token (native NEAR USDC), is
immutable (no `migrate`, no upgrade path) and its account has **no access keys** after deploy.

> **Disclosure (NEAR HACKATHON):** "diseno y vectores escritos antes del hackaton en un repo privado (2026-10-26);
> codigo escrito desde el 2026-11-01 (SHA de inicio `3a2ebdfc832afd179298fb4f861f23409356fb01`)".
> In English: the design and the test vectors were written before the hackathon in a private repo (2026-10-26); all
> code was written from 2026-11-01 on, starting at commit `3a2ebdf`. Details in [HACKATHON.md](HACKATHON.md).

Status: v1 contract, unit tests, near-workspaces tests and testnet deployment (hackathon days 1-3). **Unaudited.**

## The problem

Agents that pay for work over x402 on NEAR today can only send money (`exact`): there is no way to hold the money
until the work is checked. Arras adds the escrow: the money is locked by the payer's own signature, the operator can
only move it to the receiver or back to the payer (never to itself), and every path ends in a pull withdrawal: if a
transfer fails (for example, the receiver is not registered in USDC), the balance stays claimable and the withdrawal
can be retried.

## How it works

| Step | Who | Call | Effect |
|---|---|---|---|
| authorize | payer (directly or through a NEP-366 relayer) | `ft_transfer_call` on USDC to Arras with a canonical `msg` | a `Payment` with a deterministic `payment_id` (base58 of sha256 of borsh) |
| release | a capturer of the operator, before `release_deadline` | `release(payment_id, amount?)` | `net` to the receiver's `claimable`, `fee` to the frozen `fee_recipient` |
| refund | a capturer or the receiver | `refund(payment_id)` | what is still locked goes to the payer's `claimable` |
| reclaim | the payer, from `reclaim_after` | `reclaim(payment_id)` | same, without the operator |
| withdraw | the holder, or anyone for them | `withdraw()` / `withdraw_for(account)` | `ft_transfer` to the holder; if it fails, the balance comes back (`+=`) |
| compact | anyone, 7 days after `reclaim_after` | `compact(payment_id)` | deletes a finished payment, keeps a tombstone, refunds its storage |

`authorize` never panics: a rejection returns the full amount to the token (`ft_resolve_transfer` answers `"0"`) and
logs `arras:rechazo:<CODE>`. Guards run in a fixed order and the first failure decides: `E01` wrong token, `E02`
paused, `E03` invalid `msg`, `E04` unknown operator, `E15` receiver is the contract or the payer, `E05` below the
operator minimum, `E06` above 10 USDC per payment, `E08`/`E09`/`E10` TVL caps per payer / operator / global
(1,000 USDC), `E11` deadlines, `E12` operator fee above the payer's signed maximum, `E13` id already used, `E14`
no storage paid. Lifecycle errors panic as `ARRAS_Enn: <message>`.

Events are NEP-297 with `standard` `arras`, version `1.0.0`, and `data` is always an array:
`authorized`, `released`, `refunded`, `reclaimed`, `withdrawn {account, token, amount, ok}`, `compacted`,
`operator_registered`, `operator_updated`, `caps_set`, `paused`, `unpaused`.

Money invariant (property-tested over 10,000 random sequences):
`sum(locked) + sum(claimable) + in_flight == tvl_global <= ft_balance_of(contract)`. It is `<=` because anyone can
`ft_transfer` tokens straight to the contract; those are not swept, on purpose.

Storage is paid by the payer through NEP-145 (`storage_deposit` on Arras), or, if the payer has none, by the
operator's sponsorship pool up to a per-payer cap. There are no allow-lists of accounts.

## Try it

| Do | See |
|---|---|
| `cargo test --workspace` (Rust from `rust-toolchain.toml`; works offline once the sandbox binary is cached) | 12 core tests (golden vectors V-1..V-3, fees F-1..F-7), 56 contract unit tests incl. U-16 (10,000 sequences), 15 sandbox tests; 3 more are `ignored` because they need network |
| `ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored` | W-13, W-13b and W-09b against Circle's real testnet USDC cloned into the sandbox |
| `cargo test -p arras-escrow-v1 --test workspaces -- --nocapture` | the `MEDIDA` lines: gas per method, minimum gas, sponsorship before `E14` |
| `env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/escrow-v1/Cargo.toml` (Docker) | `target/near/arras_escrow_v1/arras_escrow_v1.wasm`; built from commit `0fb7929` it has sha256 `811e454f6975c97d91cbd5fdb4cf3385acf000ff01f2e46a6a1840b4de5897a0` |
| `near account list-keys v1.arras-dev.testnet network-config testnet now` | an empty table: nobody can redeploy or touch the contract account |
| `near account view-account-summary v1.arras-dev.testnet network-config testnet now` | contract sha256 `811e454f...5897a0`, "Account is locked (no access keys)" |
| `near contract call-function as-read-only v1.arras-dev.testnet get_config json-args '{}' network-config testnet now` | `"version": "arras-v1"`, `tope_por_pago` 10 USDC, `tope_tvl_global` 1,000 USDC |
| `near contract call-function as-read-only v1.arras-dev.testnet contract_source_metadata json-args '{}' network-config testnet now` | NEP-330 build info: image digest and `source_code_snapshot` at commit `0fb7929` |
| `scripts/testnet.sh <step>` | the whole testnet sequence, step by step (see the header of the script) |

## Testnet deployment (2026-11-01)

**First deployment:** `v1.arras-dev.testnet` runs the reproducible wasm of commit `0fb7929`, from before the
self-review fixes. Compared with the final code it lacks `E46` (a fee recipient equal to the contract itself is
accepted), the `on_withdraw` that does not deserialize the token's return value, and `compact` for payment ids shorter
than 43 characters (about 2 in a million payments cannot be compacted; no USDC is affected). It stays as it is
(immutable, keyless). **The final deployment** goes to `arras-v1b-1101.testnet` (already funded by the faucet) with
the reproducible wasm of the reviewed commit, after the money review of this code; the authorize -> release ->
withdraw run with real USDC goes there.

| Step | Transaction |
|---|---|
| parent `arras-dev.testnet` from the faucet (10 NEAR) | `7AryAnGvomNkLmK6NrQcksKYA2Qgj7jFThazLfhPdMMX` |
| `v1.arras-dev.testnet` (5 NEAR) | `HN3LTEHvG59caCVMMkUq8zFooB1Vhn9JeHAbh2jbDj9e` |
| USDC storage for v1, worker-demo, em-fees, payer-demo | `EswxLBZ5VDjLPGec46ZdMGM91cAsrPhGfNK1chyd9C7L`, `3Li6zmHGqRXhtGgvXfqyQ7Zh3WziU2rjreJ2nQZaeb1r`, `rjVPyFnanesiJhg7GsGCBQewWRSro3iBgpV1AyfMLxB`, `44HKkBmodNZ2Fz84w6kTEKVFhtmpsckd9d1wY7WC4dDy` |
| deploy + private `new` in one transaction | `4WU6tDDCidUYhTXAnvcH3qpUyyDZc1zTWZSjYC2qX3AW` |
| operator `em` (13 % fee, no lists) | `B9ZMoLYryZ2r6a2hfEKoBReHEMBSk6rVxAknqkhMSpQT` |
| sponsorship pool 0.5 NEAR / payer NEP-145 0.05 NEAR | `BNdWM2p4fwcDthqGrNJ5ujLeSjJdkvxhhoAYPaR8dPKr` / `CbteM3u2ZAs6eiMMSeLpq7m1H56fRqwe8LJZ4wtLrkJP` |
| capturer: function-call key (`release, refund, withdraw_for`, 0.5 NEAR allowance) / its full-access key deleted | `3wSmCACjW7Me1AhEeAaGfMcMiiMteZEGrcpTHzYtgZXn` / `6VfeZ2zRbMKkjZbH2TJpeT8TUCxvGPuhxd9wjPjoZago` |
| **all keys of `v1.arras-dev.testnet` deleted** | `CgaHtGKsBC3112THLTzBDpKK8QyXWPVhJ8fCy9435vQh` |
| authorize -> release -> withdraw with real testnet USDC | pending: the payer needs USDC from Circle's faucet |

`list-keys v1.arras-dev.testnet` after the last step:

```
| # | Public Key | Nonce | Permissions |
```

## Security model

- **Immutable and keyless:** no `migrate`; `new` is `#[private]` and runs in the deploy transaction; every access key
  of the contract account is deleted. A bug is answered by pausing and deploying a new version in a new account.
- **Roles:** the owner registers and edits operators; the TVL caps (global, per operator, per payer) and the
  per-payment cap can only go down, while an operator's minimum amount and its sponsorship cap per payer can be
  changed in both directions; the guardian (or owner) pauses; only
  the owner unpauses. Neither can send funds to itself. The owner can change an operator's capturers, so it can
  choose who releases or refunds live payments, but the destinations (receiver, payer, the frozen fee recipient) do
  not change. Pause only blocks new payments: release, refund, reclaim, withdraw and compact keep working.
- **Frozen terms:** the fee and the fee recipient are copied into each payment when it is authorized.
- **Caps:** 10 USDC per payment and 1,000 USDC global TVL, fixed at `new` and never raisable; per-operator and
  per-payer TVL caps and a minimum amount per operator.
- **Pull model:** no promise in release/refund/reclaim; each withdrawal has its own callback with static gas, so a
  failed transfer (an unregistered receiver, for example) only puts that balance back.
- **Not mitigated, declared:**
  - Circle can blocklist or pause the contract's USDC account, and can upgrade its token contract. If Circle
    blocklists a receiver or a fee recipient, its `claimable` balance cannot be withdrawn (every retry fails) and
    keeps counting in `tvl_global`; there is no way to redirect it.
  - Tokens sent directly to the contract (not through `ft_transfer_call`) are not recoverable, by design.
  - The TVL caps can be filled on purpose by anyone with the money to lock, and that money is not lost: it can be
    recovered. The per-payer and per-operator caps count only what is still locked in open payments, so release,
    refund and reclaim free them; several payer accounts can fill an operator's cap (`E09` for its payers) while
    their payments stay open. The global cap (1,000 USDC, fixed forever) also counts what was already released,
    refunded or reclaimed but not yet withdrawn (`claimable`, and withdrawals in flight): it only goes down when a
    withdrawal succeeds. A single account can therefore keep it full, and every new payment gets `E10`, until that
    balance is withdrawn. Anyone can call `withdraw_for` and clear it, unless the holder cannot receive USDC (for
    example, it is not registered in the token or Circle blocks it); then the global cap stays full and v1 cannot be
    changed. Open payments keep working while a cap is full: release, refund, reclaim and withdraw do not check it.
  - Sybil payers without NEP-145 can use up the operator's sponsorship pool (each up to its per-payer cap); then
    payers without their own NEP-145 deposit get `E14`. The pool cannot be withdrawn, only spent or refilled.
  - Operator registration and edits are paid from the contract's free reserve, so the owner is trusted not to
    exhaust it (for example with a huge capturer list); `get_storage_health` reports the reserve.
  - A `payment_id` is the base58 of a 32-byte hash: usually 43 or 44 characters, but it can be shorter (down to 32).

## Repository

| Path | What |
|---|---|
| `contracts/escrow-core` | `payment_id`, canonical `msg`, fees, error codes and events (no contract state) |
| `contracts/escrow-v1` | the v1 contract; `src/unit_tests/` (U-*), `tests/workspaces.rs` (W-*, U-24), `tests/fixtures/` |
| `contracts/test-ft` | near-contract-standards fungible token, only for the sandbox tests |
| `docs/vectores.json` | golden vectors V-1..V-3 and F-1..F-7 |
| `scripts/testnet.sh` | the testnet sequence |
| `HACKATHON.md` | what existed before the hackathon and what was built during it |

Pinned: near-sdk and near-contract-standards `=5.29.1`, near-workspaces `=0.23.0`, cargo-near 0.22.0, near-cli-rs
0.30.1. `Cargo.lock` is committed (the reproducible build requires it).
