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
| `cargo test --workspace` (Rust from `rust-toolchain.toml`; works offline once the sandbox binary is cached) | 12 core tests (golden vectors V-1..V-3, fees F-1..F-7), 60 contract unit tests incl. U-16 (10,000 sequences), 15 sandbox tests; 3 more are `ignored` because they need network |
| `ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored` | W-13, W-13b and W-09b against Circle's real testnet USDC cloned into the sandbox |
| `cargo test -p arras-escrow-v1 --test workspaces -- --nocapture` | the `MEDIDA` lines: gas per method, minimum gas, sponsorship before `E14` |
| `env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/escrow-v1/Cargo.toml` (Docker) | `target/near/arras_escrow_v1/arras_escrow_v1.wasm`; checked out at commit `8da7174` (the demo contract) it has sha256 `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` |
| `near account list-keys arras-v1b-1101.testnet network-config testnet now` | an empty table: nobody can redeploy or touch the contract account |
| `near account view-account-summary arras-v1b-1101.testnet network-config testnet now` | contract sha256 `7058319d...0eada`, "Account is locked (no access keys)" |
| `near contract call-function as-read-only arras-v1b-1101.testnet get_config json-args '{}' network-config testnet now` | `"version": "arras-v1"`, `tope_por_pago` 10 USDC, `tope_tvl_global` 1,000 USDC |
| `near contract call-function as-read-only arras-v1b-1101.testnet contract_source_metadata json-args '{}' network-config testnet now` | NEP-330 build info: image digest and `source_code_snapshot` at commit `8da71748b428ceccf9fccd358e61bb4213d54e29` |
| `ARRAS_CONTRATO=arras-v1b-1101.testnet scripts/testnet.sh verificar` | local wasm (commit and sha256) next to the code hash on chain, keys and config |
| `scripts/testnet.sh <step>` | the whole testnet sequence, step by step (see the header of the script) |

## Testnet deployments (2026-11-01)

### Demo contract: `arras-v1b-1101.testnet`

The reproducible wasm of commit `8da7174` (merged into `main`), deployed after the money review of that code.
A top-level account funded by the testnet faucet; it has no access keys.

| Check | Command | Result |
|---|---|---|
| code on chain = reproducible build | `near account view-account-summary arras-v1b-1101.testnet network-config testnet now` | `Local Contract (SHA-256 checksum hex) 7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` |
| source commit (NEP-330) | `near contract call-function as-read-only arras-v1b-1101.testnet contract_source_metadata json-args '{}' network-config testnet now` | `"source_code_snapshot": "git+https://github.com/UltravioletaDAO/arras?rev=8da71748b428ceccf9fccd358e61bb4213d54e29"`, image `sourcescan/cargo-near:0.21.1-rust-1.96.0@sha256:ccb22bb4...` |
| no keys | `near account list-keys arras-v1b-1101.testnet network-config testnet now` | `\| # \| Public Key \| Nonce \| Permissions \|` (empty); summary: "Account is locked (no access keys)" |

| Step | Transaction |
|---|---|
| `arras-v1b-1101.testnet` from the faucet (10 NEAR) | `EkRBwNuPxkn1GGw4UuVbnyN9vfWfDjnFqASihkC8pZFs` |
| capturer `em-capturer-v1b.arras-dev.testnet` (0.5 NEAR) | `3dZ1SMJTqgdCsGwCm7Gap2VWhSaMhNt1S13ddftTt4P7` |
| USDC storage for the contract | `FNnkvZm8Hy1Fvmx4SfCrkujMvE2Xgx7DAQHGA4bCbk3s` |
| deploy + private `new` in one transaction | `5T6wLHkWjfxyMSaYBUbJPkJLhqJppj31bManEkcxuSr2` |
| operator `em` (13 % fee, no lists) | `2pQnJm6M5ckjpsE2L988iAc8MBYM4rgytNMSU3p28uMp` |
| sponsorship pool 0.5 NEAR / payer NEP-145 0.05 NEAR | `754m8tKiUWNptzsuJan8HVhdc9EVREBZaqo6C27fYPLN` / `XZnG8ww9ijyYCR1kYbXbTgxW6KJcXxTFeAY8XP68ND3` |
| capturer: function-call key (`release, refund, withdraw_for`, 0.5 NEAR allowance) / its full-access key deleted | `5rVXH5it7qRgMC5xqLBRXbaC5nHCEYoBcERMUvFNKfbe` / `5RrVLeaFErEyDEQ7Fq2nYgMSJS6ZWpav1vvDwN3BTXWH` |
| **all keys of `arras-v1b-1101.testnet` deleted** | `EUw6W4ioffekbBV7aJVvuLrkYSCKpB5LDxEVpJq91ejJ` |
| authorize -> release -> withdraw and authorize -> refund with real testnet USDC | below, "Payments with Circle's testnet USDC" |

The roles are shared with the first deployment: owner `owner.arras-dev.testnet`, guardian `guardian.arras-dev.testnet`,
fee recipient `em-fees.arras-dev.testnet`, payer `payer-demo.arras-dev.testnet`, worker `worker-demo.arras-dev.testnet`;
the token is the testnet USDC `3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af`. The capturer is its own
account because a function-call key only works for one contract.

### Payments with Circle's testnet USDC (2026-11-01)

Both paths ran once on the demo contract, 1 USDC each, from `payer-demo.arras-dev.testnet` (20 USDC from Circle's
faucet). The signing commands need the accounts' keys (in the near-cli-rs keychain), so only the account holders can
run them; anyone can check the result with the read-only calls of the second table.

```bash
export ARRAS_CONTRATO=arras-v1b-1101.testnet ARRAS_CAPTURER=em-capturer-v1b.arras-dev.testnet
scripts/testnet.sh 8    # authorize: ft_transfer_call of 1 USDC, msg with a random salt
scripts/testnet.sh 9    # release by the capturer, withdraw_for of the worker and of the fee recipient
scripts/testnet.sh 8    # a second authorize
scripts/testnet.sh 11   # refund by the capturer, withdraw_for of the payer
```

| Step | Transaction | `arras` event | USDC after (payer / contract / worker / fee recipient), `tvl_global` |
|---|---|---|---|
| before | | | 20 / 0 / 0 / 0, 0 |
| authorize (step 8) | `Ge2dQPopfxA3r4Ufm9hhowCkSKGTjK7QUsHX8hLN61VF` | `authorized` `H65pqeK9Ps8FQ69NsiXdi7Vs8AA2dw81dsaZw2cjTNK9`, amount `1000000`, `fee_bps` 1300 | 19 / 1 / 0 / 0, 1 |
| release (step 9) | `4yXFfhfX2weQwYbLZ8vj1gx9Tqs4Qc8iSVrLeAPizArk` | `released`, amount `1000000`, net `870000`, fee `130000` | 19 / 1 / 0 / 0, 1 (now `claimable`) |
| withdraw_for worker | `73rNxSLv9gKkX8dLYwSD9K3vfVyWMTj51XUc76af31PM` | `withdrawn` `worker-demo.arras-dev.testnet`, `870000`, `ok: true` | 19 / 0.13 / 0.87 / 0, 0.13 |
| withdraw_for fee recipient | `HbuAjzCHN7a5C6jvJDnNaU1PQqZumCAFoov8AFS3tLAy` | `withdrawn` `em-fees.arras-dev.testnet`, `130000`, `ok: true` | 19 / 0 / 0.87 / 0.13, 0 |
| authorize (step 8 again) | `2VFRw7FQ4Qut8GQoWnKgrcC1WHC871cXxWHDJyMTzRMJ` | `authorized` `HTib1a7gG2ieacAi6hFiDgWaL5pppaGUdhBxERC5pKPM`, amount `1000000` | 18 / 1 / 0.87 / 0.13, 1 |
| refund (step 11) | `455XMa5NPohRyofi7bBPgRca6Lax3jSzue7fMnrNxh35` | `refunded`, amount `1000000` | 18 / 1 / 0.87 / 0.13, 1 (now `claimable`) |
| withdraw_for payer | `4z7ctqQFuJNRWkZ6WSN9XZo8cmU2yjMVHYUa3PjWzrSU` | `withdrawn` `payer-demo.arras-dev.testnet`, `1000000`, `ok: true` | 19 / 0 / 0.87 / 0.13, 0 |

Balances were read (`ft_balance_of`, `get_claimable`, `get_config`) before and after each script step; the rows
inside steps 9 and 11 follow from their events.

| Check | Command | Result |
|---|---|---|
| the released payment | `near contract call-function as-read-only arras-v1b-1101.testnet get_payment json-args '{"payment_id":"H65pqeK9Ps8FQ69NsiXdi7Vs8AA2dw81dsaZw2cjTNK9"}' network-config testnet now` | `"status": "Released"`, `"released": "1000000"` |
| the refunded payment | `near contract call-function as-read-only arras-v1b-1101.testnet get_payment json-args '{"payment_id":"HTib1a7gG2ieacAi6hFiDgWaL5pppaGUdhBxERC5pKPM"}' network-config testnet now` | `"status": "Refunded"`, `"refunded": "1000000"` |
| nothing left in the contract | `near contract call-function as-read-only 3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af ft_balance_of json-args '{"account_id":"arras-v1b-1101.testnet"}' network-config testnet now` | `"0"`; `get_config`: `"tvl_global": "0"`, `"in_flight": "0"`, `"payments": 2` |
| the worker got the net amount | `near contract call-function as-read-only 3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af ft_balance_of json-args '{"account_id":"worker-demo.arras-dev.testnet"}' network-config testnet now` | `"870000"` (`em-fees.arras-dev.testnet`: `"130000"`) |

Two notes from running it:

- The capturer only has a function-call key, and near-cli-rs 0.30.1 online picks only full-access keys from its
  keychain (`sign_with_keychain/mod.rs` and `sign_with_legacy_keychain/mod.rs` filter on `FullAccess`). The script
  therefore signs the capturer's calls with `--offline` and the key's public key, nonce and a recent block hash read
  from the RPC, then sends the signed transaction with `near transaction send-signed-transaction`. The key stays in
  the keychain. The capturer burned about 0.31 TGas per call; its allowance went from 0.5 to 0.4975 NEAR.
- The authorize reserves its 100 TGas at the pessimistic gas price (about 0.1 NEAR), so the payer needs that much
  free NEAR. `payer-demo` had paid the NEP-145 deposit in both contracts and had 0.0996 NEAR left; the first try
  stopped in near-cli-rs before sending anything, and 0.3 NEAR from the parent fixed it
  (`5nHWbXa5xN8XdQjJSujbCYPTbRYfY4ZU1gVND4eXGq1`).

### First deployment: `v1.arras-dev.testnet`

The reproducible wasm of commit `0fb7929` (sha256 `811e454f6975c97d91cbd5fdb4cf3385acf000ff01f2e46a6a1840b4de5897a0`),
from before the self-review fixes. Compared with the demo contract it lacks `E46` (a fee recipient equal to the
contract itself is accepted), the `on_withdraw` that does not deserialize the token's return value, and `compact` for
payment ids shorter than 43 characters (about 2 in a million payments cannot be compacted; no USDC is affected). It
stays as it is (immutable, keyless); use the demo contract above.

| Step | Transaction |
|---|---|
| parent `arras-dev.testnet` from the faucet (10 NEAR) | `7AryAnGvomNkLmK6NrQcksKYA2Qgj7jFThazLfhPdMMX` |
| `v1.arras-dev.testnet` (5 NEAR) | `HN3LTEHvG59caCVMMkUq8zFooB1Vhn9JeHAbh2jbDj9e` |
| USDC storage for v1, worker-demo, em-fees, payer-demo | `EswxLBZ5VDjLPGec46ZdMGM91cAsrPhGfNK1chyd9C7L`, `3Li6zmHGqRXhtGgvXfqyQ7Zh3WziU2rjreJ2nQZaeb1r`, `rjVPyFnanesiJhg7GsGCBQewWRSro3iBgpV1AyfMLxB`, `44HKkBmodNZ2Fz84w6kTEKVFhtmpsckd9d1wY7WC4dDy` |
| deploy + private `new` in one transaction | `4WU6tDDCidUYhTXAnvcH3qpUyyDZc1zTWZSjYC2qX3AW` |
| operator `em` (13 % fee, no lists) | `B9ZMoLYryZ2r6a2hfEKoBReHEMBSk6rVxAknqkhMSpQT` |
| sponsorship pool 0.5 NEAR / payer NEP-145 0.05 NEAR | `BNdWM2p4fwcDthqGrNJ5ujLeSjJdkvxhhoAYPaR8dPKr` / `CbteM3u2ZAs6eiMMSeLpq7m1H56fRqwe8LJZ4wtLrkJP` |
| capturer `em-capturer.arras-dev.testnet`: function-call key / its full-access key deleted | `3wSmCACjW7Me1AhEeAaGfMcMiiMteZEGrcpTHzYtgZXn` / `6VfeZ2zRbMKkjZbH2TJpeT8TUCxvGPuhxd9wjPjoZago` |
| **all keys of `v1.arras-dev.testnet` deleted** | `CgaHtGKsBC3112THLTzBDpKK8QyXWPVhJ8fCy9435vQh` |

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
