# Arras

**Non-custodial escrow on NEAR for paying people, AI agents and robots.** A payer locks USDC in the contract with
one `ft_transfer_call`; an operator (Execution Market) releases it to the worker when the job is approved, or it goes
back to the payer by refund or, after a deadline, by reclaim. Version 1 holds a single token (native NEAR USDC), is
immutable (no `migrate`, no upgrade path) and its account has **no access keys** after deploy.

> **Disclosure (NEAR HACKATHON):** "diseno y vectores escritos antes del hackaton en un repo privado (2026-10-26);
> codigo escrito desde el 2026-11-01 (SHA de inicio `3a2ebdfc832afd179298fb4f861f23409356fb01`)".
> In English: the design and the test vectors were written before the hackathon in a private repo (2026-10-26); all
> code was written from 2026-11-01 on, starting at commit `3a2ebdf`. Details in [HACKATHON.md](HACKATHON.md).

Status: v1 is **live on NEAR mainnet** (`v1.arras-escrow.near`, keyless, since 2026-11-03) and Execution Market's
production deployment settles NEAR tasks through it (so far, the team's own end-to-end tests); the ERC-8004 port
(Identity and Reputation registries plus the Arras proof verifier) is live since 2026-11-04 and is Execution Market's
NEAR registry since 2026-11-06. v2 (agent budgets, disputes, operator tickets) is live on mainnet since 2026-11-08
(`v2.arras-escrow.near`, keyless; deployed in
[`FdPJxM9k...`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m)), where the team ran its smoke
test ([docs/EVIDENCE.md](docs/EVIDENCE.md), section 6); Execution Market does not use it yet. **Unaudited.**

## For NEAR HACKATHON judges

**What it is.** A non-custodial USDC escrow on NEAR for paying people, AI agents and robots for work, plus a port of
[ERC-8004](https://eips.ethereum.org/EIPS/eip-8004) ("Trustless Agents": agent identity and reputation registries) in
which a rating can carry an on-chain proof that the two sides really did business through Arras. Execution Market, a
marketplace where agents hire humans and other agents for tasks, uses it in production for NEAR tasks. The 13 mainnet
payments through v1 so far (0.10 USDC each) are the team's own end-to-end tests, with a test agent and test workers;
the 2 through v2 (0.05 USDC each) are the team's smoke test of v2.

**The problem.** Over x402 on NEAR, an agent could only pay up front (`exact`): nothing held the money until the work
was checked, and nothing tied a rating to a real deal, so reputation was free to fake. With Arras the payer's own
signature locks the USDC in a contract that pays it out only to the worker, minus the operator's fee (capped by the
payer's signed message and paid to the fee recipient fixed when the payment was authorized), or back to the payer,
with no fee on what goes back. A rating whose proof
names the Arras verifier is accepted only if an Arras payment between the two accounts released money. Plain ERC-8004
feedback without a proof stays possible, as in the standard, and readers filter by verifier.

**How NEAR is used.**

- **NEP-141 `ft_transfer_call`** on Circle's native USDC: one signed call moves the money and opens the escrow. The x402
  facilitator relays it as a **NEP-366 meta-transaction**, so the payer needs no NEAR for gas.
- **Function-call access keys**: Execution Market's capturer, `em-capturer.arras-escrow.near`, has a single key that
  can only call `release`, `refund` and `withdraw_for` on the escrow, with a 0.5 NEAR allowance. Even if it leaks, the
  money can only go where each payment already says: to the worker, with the fee to the fee recipient frozen in the
  payment, or back to the payer. (In v2, live on mainnet since 2026-11-08, an AI agent holding only a function-call
  key can spend only from a budget that a human funded and capped; in v2's smoke test the agent is the test account
  `agent-v2.arras-escrow.near` and the human is the project's funding account.)
- **Keyless, immutable contracts**: each contract was deployed together with a private `new` in a single transaction,
  then every access key on its account was deleted. There is no `migrate`, so nobody can change the code.
- **Cross-contract calls whose callbacks fail closed**: `give_feedback_with_proof` asks the Identity Registry
  (`check_agent`) and the Arras verifier, which reads the payment from the escrow (`get_payment`). If any answer is
  wrong, the rating is refused and the whole deposit goes back.
- **NEP-171 NFTs** as agent identities (ERC-8004 `agentId` = `token_id`), **named accounts and sub-accounts**
  (`erc-8004.near`, `arras-escrow.near`), **NEP-145** storage (Execution Market's sponsorship pool pays the
  storage of a payer without its own deposit, up to 0.02 NEAR each), **NEP-297** events and **NEP-330** source metadata that points at the exact commit.

**Live on NEAR mainnet.** Every transaction behind this table is in [docs/EVIDENCE.md](docs/EVIDENCE.md), with
explorer links.

| Account | What it is | Deployed (UTC) | Code (sha256 of the reproducible wasm, commit) |
|---|---|---|---|
| [`v1.arras-escrow.near`](https://nearblocks.io/address/v1.arras-escrow.near) | Arras v1 escrow: 10 USDC per payment, 1,000 USDC total, operator `em` (Execution Market, 13 % fee) | 2026-11-03 02:31 | `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada`, `8da7174` |
| [`identity-registry.erc-8004.near`](https://nearblocks.io/address/identity-registry.erc-8004.near) | ERC-8004 Identity Registry (each agent is an NFT) | 2026-11-04 21:47 | `fe284122f04f319117b47234c1daafa11830862312ad138f3463683fb498d3d5`, `f18e25c` |
| [`reputation-registry.erc-8004.near`](https://nearblocks.io/address/reputation-registry.erc-8004.near) | ERC-8004 Reputation Registry, with `give_feedback_with_proof` | 2026-11-04 21:48 | `dfe715d087517617811c1155bba9c8b6e71b23cbe63481b5afbf484bcbabd56e`, `f18e25c` |
| [`erc8004-verifier.arras-escrow.near`](https://nearblocks.io/address/erc8004-verifier.arras-escrow.near) | the proof verifier: a proof is a released Arras v1 payment between the rater and the agent | 2026-11-04 21:48 | `f1b5e29942ca32aea8416a51f3276bd728f5ab804c8853414c5f5128f276e7ba`, `f18e25c` |
| [`registry.arras-escrow.near`](https://nearblocks.io/address/registry.arras-escrow.near) | `registry-v1`, the first agent registry; Execution Market's NEAR registry until it switched on 2026-11-06 (its last rating there: 2026-11-04) | 2026-11-04 03:47 | `b7b4593025769e5a12e70ebad7a78faecf523eb834b2f01338727e2833933197`, `8cec4e6` |
| [`v2.arras-escrow.near`](https://nearblocks.io/address/v2.arras-escrow.near) | Arras v2 escrow: agent budgets, disputes with an arbiter, operator tickets; 10 USDC per payment, 1,000 USDC total, operator `em` with its own capturer and arbiter | 2026-11-08 02:20 | `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77`, `ddc5121` |

The v2 escrow is live since 2026-11-08: `scripts/mainnet-v2.sh` deployed the reproducible wasm of `ddc5121` (sha256
`1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77`) into `v2.arras-escrow.near` with a private `new`
([`FdPJxM9k...`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m)), deleted all its keys
([`Bqc63GgB...`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt)) and ran the smoke test of
the security track, with the project's funding account as the human and test accounts as the agent and the worker
([docs/EVIDENCE.md](docs/EVIDENCE.md), section 6; the checks are below). It was rehearsed twice on testnet first
(`v2.arras-v2e-1107.testnet`, then `v2.arras-v2f-1107.testnet`, which found the up-front reserve behind the script's
last fix and finished with the final script; "Arras v2 on testnet" below). Execution Market and the x402-rs facilitator
still use v1 only: pointing them at v2 is a later step
([docs/MAINNET.md](docs/MAINNET.md), section 8.9).

The run that shows it all together is Execution Market's golden flow on 2026-11-06 at 01:17Z
([docs/EVIDENCE.md](docs/EVIDENCE.md), section 5). A test agent that has only a NEAR wallet registered its identity
and locked 0.10 USDC for a task ([`Bcri1ZNj...`](https://nearblocks.io/txns/Bcri1ZNj4Agrv9QqwFneTcVMfwH1tJsKnLpmMjBijzUW):
its `ft_transfer_call` inside a NEP-366 delegate action, relayed by the facilitator `uvd-facilitator.near`). On
approval the capturer released it ([`2gkYMpHA...`](https://nearblocks.io/txns/2gkYMpHAQTUSe7NSuwUrtY3ZPzH4gLDbYUyiJBHJLhKd),
with its function-call key, relayed the same way), and two `withdraw_for` paid 0.087 USDC to the worker and 0.013 to
the fee recipient. Then each side rated the other with that payment as proof ([`EjLpmfry...`](https://nearblocks.io/txns/EjLpmfryfXJAkLhrgkntS9PjDhCMqtQyQm54a1ajn7xs),
[`JApp8fXk...`](https://nearblocks.io/txns/JApp8fXkr2nhtm7qtLqFyVfEB4YpWiBj1WVVL7dasKHW)).

**Check it yourself: read-only, no keys.** These commands use [near-cli-rs](https://github.com/near/near-cli-rs) 0.30.1
and the mainnet RPC from your own near-cli configuration. They were run against mainnet on 2026-11-06, and the results
below are what they returned. The v2 rows (the last five) are from 2026-11-08, after v2's smoke test: the last four as
near-cli returned them; the first gives what that command shows, with the facts that
`scripts/mainnet-v2.sh --mainnet-ok verificar` read through the RPC.

| Do | See |
|---|---|
| `near account view-account-summary v1.arras-escrow.near network-config mainnet now` (also for the three ERC-8004 contracts and `registry-v1` of the table; v2 below) | `Local Contract (SHA-256 checksum hex)` = the sha256 of the table; `Access keys: Account is locked (no access keys)`. The "Delegated stake" line prints a handler error from `poolv1.near`, unrelated to the contract |
| `near account list-keys v1.arras-escrow.near network-config mainnet now` (same for the ERC-8004 contracts and `registry-v1`) | an empty table, the same fact as the `Access keys` line of `view-account-summary` above: nobody can redeploy, upgrade or sign for the contract |
| `near contract call-function as-read-only v1.arras-escrow.near contract_source_metadata json-args '{}' network-config mainnet now` | NEP-330: `source_code_snapshot` `git+https://github.com/UltravioletaDAO/arras?rev=8da71748b428ceccf9fccd358e61bb4213d54e29`, image `sourcescan/cargo-near:0.21.1-rust-1.96.0@sha256:ccb22bb4...` (the ERC-8004 contracts: `rev=f18e25c4...`; `registry-v1`: `rev=8cec4e64...`). Its `build_command` reads `non-reproducible-wasm`: that is the `container_build_command` of each `Cargo.toml`, the command cargo-near runs inside that pinned image during a reproducible build |
| `near account list-keys em-capturer.arras-escrow.near network-config mainnet now` | one key: `only do ["release", "refund", "withdraw_for"] function calls on v1.arras-escrow.near with an allowance of 0.5 NEAR` |
| `near contract call-function as-read-only v1.arras-escrow.near get_config json-args '{}' network-config mainnet now` | `"version": "arras-v1"`, `tope_por_pago` `10000000` (10 USDC), `tope_tvl_global` `1000000000` (1,000 USDC), the USDC token |
| `near contract call-function as-read-only v1.arras-escrow.near get_payment json-args '{"payment_id":"DAtTVvHHPTfUbneHj1dUYi1mG3aqE8GbEK5aQzVExZ2X"}' network-config mainnet now` | the golden flow's payment: `"status": "Released"`, `amount` and `released` `100000` (0.10 USDC), `operator_id` `em`, `fee_bps` 1300 |
| `near contract call-function as-read-only identity-registry.erc-8004.near nft_token json-args '{"token_id":"1"}' network-config mainnet now` | agent 1, owned by the paying agent `fe0a5742...1079`, `reference` = its Execution Market profile |
| `near contract call-function as-read-only reputation-registry.erc-8004.near read_feedback json-args '{"agent_id":"0","client_address":"fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079","feedback_index":"2"}' network-config mainnet now` | the agent's rating of the worker (agent 0): `"value": "90"`, `tag2` `execution-market` |
| the same arguments with `read_feedback_proof` | `"proof": "DAtTVvHH..."` (the payment above), `"verifier": "erc8004-verifier.arras-escrow.near"`, `"role": "receiver"`, `"context": "em"`, `"weight": "100000"` (the USDC released) |
| `read_feedback` / `read_feedback_proof` with `'{"agent_id":"1","client_address":"826e8c640ba0ee77bf255ecb332dfaca73fc1e3aef2dbf370b5ffeb30f9ac7af","feedback_index":"1"}'` | the worker's rating of the agent: `"value": "85"`, the same payment as proof, `"role": "payer"` |
| `near account view-account-summary v2.arras-escrow.near network-config mainnet now` | SHA-256 `1541525c...fa77`; no access keys. The same facts that `scripts/mainnet-v2.sh --mainnet-ok verificar` read on 2026-11-08: `code_hash` `2RyLHaUjZAXZzwEbnztZeJNB73wJqstjZpY7rtSVx44e` (the base58 of that sha256) and an empty `view_access_key_list` |
| `near contract call-function as-read-only v2.arras-escrow.near get_operator json-args '{"operator_id":"em"}' network-config mainnet now` | `"capturers": ["em-capturer-v2.arras-escrow.near"]`, `"arbiter": "em-arbiter-v2.arras-escrow.near"`, `arbiter_window_ns` `604800000000000` (7 days), `fee_bps` 1300, `fee_recipient` `em-fees.arras-escrow.near`, `min_amount` `10000` (0.01 USDC), `"ticket_pubkey": "ed25519:9zh27mQwBQbKGYixfqUVEU4xNtz93Sad8crhhFH4B9YF"`, `sponsor_pool` `100000000000000000000000` (0.1 NEAR), `tvl_operador` `0` |
| `near account list-keys agent-v2.arras-escrow.near network-config mainnet now` | one key, `ed25519:3TJqivtjEskrdMQTQ8LWpdQcFNN6rFdZpNyGwAfxvdFU`: `only do ["authorize_from_budget"] function calls on v2.arras-escrow.near with an allowance of 0.0591054940912723 NEAR` |
| `near contract call-function as-read-only v2.arras-escrow.near get_payment json-args '{"payment_id":"9nySy6vTK95bgPUm5MkdsZtzC9XaGrSCV1EjNnRR6uyC"}' network-config mainnet now` | the smoke's first payment: `"status": "Released"`, `amount` and `released` `50000` (0.05 USDC), `refunded` `0`, `payer` the funding account `fe0a5742...1079` (the human), `receiver` `worker-v2.arras-escrow.near`, `operator_id` `em`, `fee_bps` 1300, `"dispute": null` |
| the same with `'{"payment_id":"8428jJdxtdVfJwrMKc7ircLp1Yr6D5nra5e16ZbRNS8w"}'` | the disputed one: `"status": "Resolved"`, `amount` and `refunded` `50000` (all of it back to the payer), `released` `0`, `dispute.opened_by` `Payer` |

To rebuild the code and compare hashes: `env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path
contracts/<crate>/Cargo.toml` (Docker), with `escrow-v1` at commit `8da7174`, `registry-v1` at `8cec4e6`, and
`erc8004-identity`, `erc8004-reputation` and `erc8004-arras-verifier` at `f18e25c`. Each wasm was built twice from
clean clones before its deployment, with the same sha256 both times. `escrow-v2` at `ddc5121` gives
`1541525c...fa77`, the wasm that `scripts/mainnet-v2.sh` pins and deployed (built twice from clean clones under Linux on
2026-11-07, and again on macOS by `scripts/mainnet-v2.sh wasm` before the mainnet run: the same sha256). Execution
Market's own code is not in this repository; its use of the contracts can be seen on chain.

**Built during the hackathon.** The repository was created on 2026-11-01. The hackathon work is everything after the
start commit `3a2ebdf`:
[3a2ebdf...main](https://github.com/UltravioletaDAO/arras/compare/3a2ebdfc832afd179298fb4f861f23409356fb01...main).
[HACKATHON.md](HACKATHON.md) lists what existed before: the x402-rs facilitator's NEAR `exact` payments,
Execution Market on EVM networks, and the Arras design and test vectors. It also lists what was deployed and
when. The pinned libraries are listed under "Repository" below.

## The problem

Agents that pay for work over x402 on NEAR today can only send money (`exact`): there is no way to hold the money
until the work is checked. Arras adds the escrow: the money is locked by the payer's own signature, the operator can
only move it to the receiver, minus its fee (no higher than the payer signed for, paid to the fee recipient frozen in
the payment when it was authorized), or back to the payer, and every path ends in a pull withdrawal: if a
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

## Arras v2: agent budgets, disputes and operator tickets

`contracts/escrow-v2` is v1 (same money rules, same events, same `payment_id`) plus three things. It is a separate,
immutable contract in its own account; `arras-escrow-core` and v1 are not touched (see "Repository").

**Agent budget (the AI-security path).** A human deposits USDC for one agent account with the same `ft_transfer_call`,
`msg` `{"v":1,"action":"budget","operator_id":"em","agent":"<account>","tope_por_pago":"2000000","tope_diario":"10000000"}`.
The agent then calls `authorize_from_budget(owner, operator_id, receiver, amount, release_deadline, reclaim_after,
dispute_deadline?, fee_bps_max, terms_hash, salt, operator_ticket?)`. That method takes no deposit, so the agent signs
it with a **function-call access key limited to this contract and this method, with no full-access key at all**. What
a leaked agent key can do is bounded by the contract, not by the agent:

| Limit | Enforced by |
|---|---|
| per payment: the budget's cap (`E52`) and the contract's 10 USDC (`E06`) | `authorize_from_budget` |
| per day (UTC day window): the budget's daily cap (`E51`), and the balance (`E50`); no zero-amount payments (`E24`) | `authorize_from_budget` |
| horizon: `reclaim_after` at most 60 days ahead (`E57`; an Execution Market task of up to 30 days needs 44), so the human gets back, with `reclaim`, whatever the agent locked and nobody released, as long as the human does not open a dispute, or opens it at or after `release_deadline` (see Disputes) | `authorize_from_budget` |
| the agent cannot shorten the human's dispute window: `dispute_deadline` no earlier than `release_deadline` (`E67`) | `authorize_from_budget` |
| the money stays in escrow: only the operator's capturer releases it to the receiver; a refund or reclaim goes back to the **human** (the payment's payer), never to the agent | the v1 lifecycle |
| receiver cannot be the contract, the human or the agent (`E15`) | `authorize_from_budget` |
| raising caps needs the human's full-access key (`set_budget_caps`, 1 yoctoNEAR); a re-deposit only lowers them | `set_budget_caps`, the `budget` msg |
| the human takes the rest back at any time, even while paused: `withdraw_budget`, or `revoke_agent` (also deletes the budget and refunds its storage) | owner-only methods |

The agent's payment has `payer = owner`, and its `payment_id` is the v1 formula with the owner as payer
(`compute_budget_payment_id`; vector VB-1).

**Disputes.** The payer or the receiver can `open_dispute(payment_id, reason_hash)` while the payment still holds money
and before its `dispute_deadline` (an optional field of the authorize `msg`; it defaults to `reclaim_after`, so a v1
`msg` is a valid v2 `msg` with the same `payment_id`; if given, it cannot be earlier than `release_deadline`, `E67`). The money left in the payment is frozen: release, refund and
reclaim answer `E22`. Only the operator's arbiter splits it with `resolve_dispute(payment_id, to_receiver, to_payer)`
(the sum must be exactly what is left; the frozen fee is charged on `to_receiver`), within the arbiter window frozen in
the payment at authorization (`[1 minute, 30 days]`). If the arbiter does not answer in time, anyone calls
`resolve_expired_dispute`: what was released stays released and what is left goes to **the party that did not open the
dispute**, so opening one to block a finished job (or a refund) is not free. One exception, the last agreed state: if
the payer opened it at or after `release_deadline`, when the capturer could no longer release, what is left goes back to
the payer. So with a leaked agent key the human waits for `reclaim`, asks the capturer for a refund, or disputes from
`release_deadline` on, never before. The storage a dispute adds (86 bytes) is charged when the payment is authorized
and refunded on `compact` if no dispute was opened.

**Operator ticket.** If the operator registers a `ticket_pubkey` (ed25519; never a small-order point, nor the published test key of the vectors), every payment and every budget deposit in
that operator needs its signature: `operator_ticket: {"expiry":"<ns>","sig":"<base58>"}` over
`borsh("arras:v2:operator_ticket", contract account, subject, expiry)`, where the subject is the `payment_id` or the
`(operator_id, owner, agent)` of a budget. Throwaway accounts can no longer fill the TVL caps or drain the sponsorship
pool. The views `ticket_message_payment` and `ticket_message_budget` return the exact bytes to sign; vectors T-1 and T-2
in `docs/vectores.json`. A payment ticket is single-use in practice (a `payment_id` never repeats, `E13`); a budget
ticket signs no amount and no nonce, so it admits any number of deposits for that `(owner, agent)` pair until `expiry`,
also after `revoke_agent`: operators sign budget tickets with a short `expiry`.

The owner can rotate the ticket key or remove it with `update_operator` (1 yoctoNEAR). Removing it opens the operator
to payments and budget deposits without a ticket again, for an operator such as Execution Market before it signs
tickets:

```bash
near contract call-function as-transaction <v2 contract> update_operator json-args '{"operator_id":"em","patch":{"quitar_ticket_pubkey":true}}' prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as <owner> network-config mainnet sign-with-keychain send
```

(`"patch":{"ticket_pubkey":"ed25519:..."}` sets or rotates it; sending both is `E48`.)

New errors: `E47` arbiter window, `E48` ticket key (ed25519 only, no small-order point), `E50` no budget or balance,
`E51` daily cap, `E52` budget per-payment cap, `E53` missing or invalid ticket, `E54` expired ticket, `E55` invalid
agent, `E56` zero caps, `E57` budget payment beyond the 60-day horizon, `E60` not a party, `E61` dispute deadline
passed, `E62` not the arbiter, `E63` split does not add up, `E64` arbiter still in time, `E65` arbiter window passed,
`E66` invalid `reason_hash`, `E67` `dispute_deadline` before `release_deadline`. New events: `budget_deposited`, `budget_spent`, `budget_withdrawn`, `budget_caps_set`,
`agent_revoked`, `dispute_opened`, `dispute_resolved`.

Money invariant of v2 (property-tested over 10,000 random sequences with budgets, disputes and interleaved
withdrawals): `sum(in escrow, disputed included) + sum(claimable) + in_flight + sum(budgets) == tvl_global <=
ft_balance_of(contract)`, and what came in through `ft_transfer_call` equals what went out plus what the contract holds.

## Agent registry: identity and reputation on NEAR (`contracts/registry-v1`)

The shape of ERC-8004 (Identity Registry + Reputation Registry) on NEAR, with reputation tied to Arras payments.

- **Identity is the NEAR account itself** (no NFT, no ids): `register(agent_uri, metadata?)` and `update(...)` for the
  calling account, each paying its own storage (measured; the rest of the deposit comes back).
- **A rating needs a real deal.** `give_feedback(escrow, payment_id, score 0..100, tag1?, tag2?, feedback_uri?,
  feedback_hash?)` is checked on chain, in the callback of a cross-contract call to `escrow.get_payment`: the payment
  exists and released money (`released > 0`, in any state), the caller is its payer or receiver, and the rated account
  is the other party. One rating per `(escrow, payment_id, rater)`, locked while the call is in flight. `escrow` must
  be in the contract list fixed in `new` (`v1.arras-escrow.near` on mainnet): a list of contracts, not of people.
- The callback never panics and fails closed: the `get_payment` answer must be exactly v1's `Payment` (its 16 fields,
  no more and no fewer, one of v1's 5 statuses, numbers as canonical decimal strings, `operator_id` in v1's charset),
  or it is rejected. A rejection refunds the whole deposit, logs `arras-registry:rechazo:<CODE>` and returns
  `null`. The rater can revoke its own rating (kept, marked). Views: ratings received (paginated), a summary (count,
  average, last; also split by role) and the same summary per operator, `get_summary_by_operator(account,
  operator_id)`. Each rating stores the payment's `operator_id` and `released`. NEP-297 events `arras-registry` 1.0.0.
- No upgrade path: deployed with a private `new` in the same transaction and then left without keys, like v1.
- Trust: a rating is as good as the escrow's payments. In v1 the owner registers operators and each operator's
  capturer releases, so the global summary mixes operators: Execution Market must read
  `get_summary_by_operator(account, "em")` and weigh `list_feedback` by `released` and distinct raters. An indexer
  counts only `feedback_given`/`feedback_revoked` events from receipts executed by the registry account itself and
  whose final status is a success: any contract can log the same `EVENT_JSON`, and a receipt that logs and then fails
  keeps the log while its state is reverted (sandbox test WR-10; the handoff spells out the model).

Interface, events, measured gas and the exact deployment commands: `docs/handoffs/registry-v1.md`. Live on mainnet
at `registry.arras-escrow.near` since 2026-11-04 (keyless; [docs/EVIDENCE.md](docs/EVIDENCE.md), section 3).
Execution Market's NEAR ratings went there until its configuration switched to the ERC-8004 registries below on
2026-11-06; its last rating there is from 2026-11-04 at 08:04Z
([`DV8a9kd1...`](https://nearblocks.io/txns/DV8a9kd1aHHHnUbEji7fGR4wjdBr2nF71gMykGNFPuid)).

## ERC-8004 on NEAR (Identity, Reputation and Validation registries)

A faithful port of [ERC-8004](https://eips.ethereum.org/EIPS/eip-8004) ("Trustless Agents", reference implementation
2.0.0 of `erc-8004/erc-8004-contracts`) to NEAR, in new crates; `contracts/registry-v1` and its live deployment are not
touched. Plan and the full EVM -> NEAR table: [docs/plans/erc8004-near-port.md](docs/plans/erc8004-near-port.md);
interface, deploy plan and what was run: [docs/handoffs/erc8004-near.md](docs/handoffs/erc8004-near.md). **Live on
mainnet since 2026-11-04** (testnet first, the same day), immutable and keyless; the transactions are in
[docs/EVIDENCE.md](docs/EVIDENCE.md), section 4. Validation is not deployed. **Unaudited.** The accounts under
`erc-8004.near` are this project's deployment, not an official one of the ERC-8004 team, which asks for a single
deployment per chain coordinated with it.

| Crate | Account (mainnet) | What it is |
|---|---|---|
| `contracts/erc8004-identity` | `identity-registry.erc-8004.near` | each agent is an NFT (NEP-171/177/178/181): `agentId` = `token_id`, `agentURI` = `metadata.reference`, on-chain metadata key -> bytes, `agentWallet` (the new wallet confirms by calling the contract itself; cleared on every transfer; a pending one is cancelled by any approval revocation) |
| `contracts/erc8004-reputation` | `reputation-registry.erc-8004.near` | open feedback (`value`/`valueDecimals`, tags, endpoint, `revokeFeedback`, `appendResponse`, `getSummary` with a mandatory client filter); the "agent exists and the rater is not its owner or an approved operator" check is a cross-contract call whose callback fails closed and refunds the whole deposit. Optional extension (cargo feature `feedback-proofs`, on by default): `give_feedback_with_proof` with an on-chain proof from any verifier contract |
| `contracts/erc8004-arras-verifier` | `erc8004-verifier.arras-escrow.near` | the Arras proof verifier: one escrow fixed at init (`v1.arras-escrow.near`); a proof is a released Arras payment between the rater and the agent's owner or `agentWallet` |
| `contracts/erc8004-validation` | `validation-registry.erc-8004.near` (account reserved) | **draft, not deployable**: the ERC-8004 team has not finalized the Validation section |
| `contracts/erc8004-common` | - | shared validation, canonical integers, the 256-bit summary, NEP-297 envelope and the cross-contract schemas |

All are immutable (private `new` in the deploy transaction, no `migrate`, no owner, keys deleted). Events are NEP-297
`erc8004` 1.0.0 (`registered`, `metadata_set`, `uri_updated`, `agent_wallet_pending`, `new_feedback`, `feedback_revoked`, `response_appended`,
`validation_request`, `validation_response`) plus NEP-171 `nft_mint`/`nft_transfer`. The NEAR `agentRegistry` in a
registration file is `near:mainnet:identity-registry.erc-8004.near` (CAIP-2 has no `near` namespace registered yet).

## Try it

| Do | See |
|---|---|
| `cargo test --workspace` (Rust from `rust-toolchain.toml`; works offline once the sandbox binary is cached, or with `NEAR_SANDBOX_BIN_PATH` pointing at a `near-sandbox` 2.13.4 binary) | v1: 12 core tests (golden vectors V-1..V-3, fees F-1..F-7), 60 contract unit tests incl. U-16 (10,000 sequences), 15 sandbox tests, 3 more `ignored` because they need network; v2: its unit tests incl. its own U-16 (10,000 sequences) and the vectors read from `docs/vectores.json`, and 11 sandbox tests (W2-01..W2-10, U-24); the registry and the ERC-8004 port: their unit and sandbox tests (next rows) |
| `cargo test -p arras-escrow-v2 --test workspaces -- --nocapture` | the v2 `MEDIDA` lines: gas per method (W2-09), minimum callback gas (W2-06) |
| `scripts/test_mainnet.sh` (offline: `near`, `aws` and the RPC are test doubles; v2's run against a stateful fake chain, `scripts/test_cadena.py`, that refuses any transaction not signed by a live key with the right permission) | the guards of `scripts/mainnet.sh` and `scripts/mainnet-v2.sh`, under every `bash` installed (macOS 3.2 and 5.x); `ARRAS_TEST_SOLO=v2` runs v2's only |
| `scripts/mainnet.sh plan` / `scripts/mainnet.sh todo` | the mainnet deployment as a dry run: accounts, amounts, caps, the pinned wasm and every command, with no network call |
| `scripts/mainnet-v2.sh plan` / `todo` / `humo` | the same for v2: its budget in NEAR step by step, every command of the deployment and of the smoke test, with no network call |
| `ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored` | W-13, W-13b and W-09b against Circle's real testnet USDC cloned into the sandbox |
| `cargo test -p arras-escrow-v1 --test workspaces -- --nocapture` | the `MEDIDA` lines: gas per method, minimum gas, sponsorship before `E14` |
| `cargo test -p arras-registry-v1` | the registry: 115 unit tests (every guard, the callback with each `get_payment` result and every way an answer can differ from v1's `Payment`, the lock, caller vs signer, storage of the worst case) and 14 sandbox tests (WR-00..WR-10; WR-02..WR-04 twice: against v1 built here and against the exact mainnet wasm of `v1.arras-escrow.near` in `contracts/registry-v1/tests/fixtures`, sha256-checked) with real Arras v1 payments, a mock escrow and an allowlisted account without code |
| `cargo test -p erc8004-common -p erc8004-identity -p erc8004-reputation -p erc8004-arras-verifier -p erc8004-validation` (and `-p erc8004-reputation --no-default-features --lib` for the pure port) | the ERC-8004 port: every guard, every result of every callback, the worst-case storage and log size of each call, the token reserve invariant, and 14 sandbox tests (WE-00..WE-12; WE-05 twice, against v1 built here and against the exact mainnet wasm) with the three contracts and real Arras v1 payments. `-- --nocapture` on `--test workspaces` prints the `MEDIDA` lines (gas per call, storage per account) |
| `cargo test -p arras-registry-v1 --test workspaces -- --nocapture` | the registry `MEDIDA` lines: gas per receipt of `give_feedback`, worst-case callback, storage per rating |
| `env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/escrow-v1/Cargo.toml` (Docker) | `target/near/arras_escrow_v1/arras_escrow_v1.wasm`; checked out at commit `8da7174` (the demo contract) it has sha256 `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` |
| `near account list-keys arras-v1b-1101.testnet network-config testnet now` | an empty table: nobody can redeploy or touch the contract account |
| `near account view-account-summary arras-v1b-1101.testnet network-config testnet now` | contract sha256 `7058319d...0eada`, "Account is locked (no access keys)" |
| `near contract call-function as-read-only arras-v1b-1101.testnet get_config json-args '{}' network-config testnet now` | `"version": "arras-v1"`, `tope_por_pago` 10 USDC, `tope_tvl_global` 1,000 USDC |
| `near contract call-function as-read-only arras-v1b-1101.testnet contract_source_metadata json-args '{}' network-config testnet now` | NEP-330 build info: image digest and `source_code_snapshot` at commit `8da71748b428ceccf9fccd358e61bb4213d54e29` |
| `ARRAS_CONTRATO=arras-v1b-1101.testnet scripts/testnet.sh verificar` | local wasm (commit and sha256) next to the code hash on chain, keys and config |
| `ARRAS_CONTRATO=arras-v1b-1101.testnet ARRAS_CAPTURER=em-capturer-v1b.arras-dev.testnet scripts/testnet.sh <step>` | the whole testnet sequence on the demo contract, step by step (see the header of the script) |

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
account because the first deployment's capturer (`em-capturer.arras-dev.testnet`) has no full-access key since its
step 7, so it cannot add (`AddKey`) a function-call key for another contract.

### Payments with Circle's testnet USDC (2026-11-01)

Both paths ran once on the demo contract, 1 USDC each, from `payer-demo.arras-dev.testnet` (20 of Circle's testnet
USDC). The signing commands need the accounts' keys (in the near-cli-rs keychain), so only the account holders can
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
  the keychain. Each capturer call burned 2.9-7.3 TGas (release and refund about 3, withdraw_for about 7.3);
  its allowance went from 0.5 to 0.4975 NEAR.
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

## Arras v2 on testnet: `arras-v2-1103.testnet` (2026-11-03)

The reproducible wasm of commit `c416989` (`env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path
contracts/escrow-v2/Cargo.toml`, built twice, the second time from a clean clone: same hash), deployed with a private
`new` in the same transaction to a top-level account funded by the testnet faucet. All its keys are deleted.

This is the pre-review deployment. The contract code after `c416989` (`git diff c416989 HEAD -- contracts/escrow-v2/src`
shows it, plus tests) changes three things: the text of `E57` said "30 dias" while the limit was already 60 days; a
dispute the payer opens at or after `release_deadline` now goes back to the payer when it expires (the money review's
P2-1); and `dispute_deadline` cannot be earlier than `release_deadline` (`E67`). The walkthrough below would end the
same on the new code: the expired dispute (step 11) was opened before `release_deadline`, and the `msg` of step 10 sets
`dispute_deadline` equal to `release_deadline`. As with v1, the reviewed commit is deployed to a new keyless account
after the money review.

| Check | Command | Result |
|---|---|---|
| code on chain = reproducible build | `near account view-account-summary arras-v2-1103.testnet network-config testnet now` | `Local Contract (SHA-256 checksum hex) 8ef765f9f6c3b967cd55501f0eb3c8856b24085acabb479745624b9682f5d66a` (366,095 bytes) |
| source commit (NEP-330) | `near contract call-function as-read-only arras-v2-1103.testnet contract_source_metadata json-args '{}' network-config testnet now` | `source_code_snapshot ...?rev=c41698900a78dc50b365e9cfeb761ddf40e7511b` |
| no keys | `near account list-keys arras-v2-1103.testnet network-config testnet now` | empty table |
| the agent has only a function-call key | `near account list-keys agent.arras-v2-1103.testnet network-config testnet now` | one key: `only do ["authorize_from_budget"] function calls on arras-v2-1103.testnet with an allowance of 0.25 NEAR` |
| config | `near contract call-function as-read-only arras-v2-1103.testnet get_config json-args '{}' network-config testnet now` | `"version": "arras-v2"`, 10 USDC per payment, 1,000 USDC global TVL |
| everything at once | `scripts/testnet-v2.sh verificar` | local wasm (commit, sha256) next to the code hash on chain, keys and config |

Roles: owner `owner.arras-dev.testnet`, guardian `guardian.arras-dev.testnet`, fee recipient `em-fees.arras-dev.testnet`,
payer (the human) `payer-demo.arras-dev.testnet`, worker `worker-demo.arras-dev.testnet` (the same as v1); capturer
`capturer.arras-v2-1103.testnet`, arbiter `arbiter.arras-v2-1103.testnet` and agent `agent.arras-v2-1103.testnet`,
sub-accounts created by the contract account before its keys were deleted. Two operators: `em` (no ticket) and `emt`
(with a `ticket_pubkey` generated locally for the demo; its private key never left `~/.arras-testnet`). Both use the
minimum arbiter window, 60 seconds, so the expired-dispute path can be shown live; a production operator would use
days.

| Step (`scripts/testnet-v2.sh <n>`) | Transaction |
|---|---|
| 1. `arras-v2-1103.testnet` from the faucet (10 NEAR) | `DW8fn7gg37mSChMohRSCHpVMVNTr7exSG8rhKzuirrwf` |
| 2. capturer / arbiter / agent sub-accounts | `6eJSrG6zCD6JENk1PHd5zcS7FRiRiqfxd7PeJ1JiSXv4` / `B9XdgqWQi6qgYBm3pvdqxfxhXRPRuA8ALqaALpAcHj53` / `C85bKtZSvULkVmjLprRs95jr2y6hA1dKCydJLxqCHFKD` |
| 3. USDC storage for the contract | `5288pPdAMCD3v9RqBpvgJkWNBqxkKKwBUJeSMQhZvygX` |
| 4. deploy + private `new` | `E6xN7eKajKcXc7pXUTzN2YMhKcWdhtM5FcAdjKu7NADC` |
| 5. operators `em` / `emt` (with ticket key); payer NEP-145 0.05 NEAR | `4HGQ7afZ1WN69BK9SvVXTtBZXT3Xp7xUpPevJgu4ouks` / `E1ghBMR8nWn9TaLBZjaG83VqY3mpHsuDyrEweoqE4y7q`; `9Kxvwq2y4MGMjfX6JkrZwcMqE2cX3XN1M9Qfd7paE7rG` |
| 6. capturer: function-call key (`release, refund, withdraw_for`) / full-access key deleted | `5NtuFuVA95n9qEZyqY6pCpmeomfzgCRM2VK7MkGYhinx` / `3V88AoGeGPC1YJHPryEnEymRviEdnMYJfgMubLUEaTdQ` |
| 6. agent: function-call key (`authorize_from_budget`) / full-access key deleted | `CUSj9SiAPnsSBfYbj1oL5ho5PtnheoAZhwYsSUHZzZqs` / `5mWTATiSaVdFkXnsmRhCbpa4gt3mmEpLd2aVYwjevavz` |
| 7. **all keys of `arras-v2-1103.testnet` deleted** | `29wLJFBt8av41bEudrzmqB2KZ55bZ4zpDT9KsHv3h43o` |

### The walkthrough with Circle's testnet USDC (2026-11-03, 00:48Z to 01:05Z)

Everything below ran against the keyless contract. Balances (USDC) before: payer 17.0, worker 2.61, fee recipient
0.13, contract 0.

| Step | What happened | Transactions | Events |
|---|---|---|---|
| 8. budget | the human deposits 2 USDC for the agent (caps: 1 per payment, 2 per day) | `HRcu5ddCC5HzxUvta1rW3unDE814wHc32xhUT9AVnqqz` | `budget_deposited`; `tvl_global` = `budgets_total` = 2 |
| 9. agent pays | the agent, signing with its function-call key, authorizes 1 USDC to the worker (`authorize_from_budget`); the capturer releases; `withdraw_for` worker and fee recipient | `A6VML6yHM94bmLgjvq37fYj7otyVd53MfszWC1pWUzGr`; `uNCnj5xGQrY6XgvB5Aabx6UdXpe72zWigSpFM1hzizS`; `83u7hRpiyHeAa871WXY6srvq7zCPV6MD8nnNBdWJByRV`, `27GBWqrhGL6QmmuAmvd6xku58kxCMFuBFbS6ZympxyXx` | `authorized` (payer = the human) + `budget_spent`; `released` 0.87 / 0.13; `withdrawn ok:true` x2 |
| 10. dispute, arbiter | the human pays 0.5 USDC by `ft_transfer_call` (v2 `msg` with `dispute_deadline`), opens a dispute, the arbiter gives 0.2 to the worker and 0.3 back; withdrawals | `9zHVHSS8fu2PTAWvzYjtousunYDvKLunmKh1si5VuNpw`; `ACwULbvxfgVgT6aMkF47Vwa8GyYancKUAxGXs1z7PG2J`; `FGapXa3SHsQb4GJNvTfMRSEtSCeWGnt2n2EmzyLeaAX3`; `H7mqAso3Ak5Wray3JwfwPkKCtpPBuq6jsM258eJp9k54`, `E5KFYZXgeKtsvN4NCELKUhVZKGiqW3qhs1cNEoJ55U61`, `EAT7tovfRk8698WuCSq3kuJJ5E8eAcCKX9RtbpnhFt7Z` | `dispute_opened`; `dispute_resolved` (net 0.174, fee 0.026, to payer 0.3, `expired:false`) |
| 11. dispute, expired | the agent authorizes 0.5 USDC; the human opens a dispute; the arbiter does not answer; 60 s later the human (anyone could) closes it and what was left goes to the worker, the party that did not open it | `EHsnaQPVdQ2CmqSVWPMKPcdrPJZVG7RBgN1HaFiZzqiZ`; `Cnckf48GTXtgcbPUcKLM5gD2QmwpvwdquzxUW24ZvN3Q`; `4n2V7cEJCkcfKtEMuWrqWBfrYi6tJRAfUx9ZWmh1pjRF`; `6bQCstaMYKnUwDee57KWAyXirs8QfbVb2TRAvMWVByJX`, `DcMXUiWMMDSkwxcP7nJFcjqYnoiiGFvBbw8PvdY8zKS6` | `dispute_resolved` (to receiver 0.5: net 0.435, fee 0.065, `expired:true`) |
| 12. ticket | operator `emt`: 0.1 USDC without a ticket comes back whole; with the ticket signed over the bytes of `ticket_message_payment` it is authorized; the capturer refunds; withdrawal | `FZgVdueMkLPCPu1otU888iM29XgHTUS8EUKhsYhiwrga`; `4x6ZytxfoDmnQa7gsCPcrDoCT8pG4pBHfjuFKdcRqNY4`; `FtjM1rnxC79tcuu7hBMc7KZJSSrRXfeEJEGXW17jYQXw`; `tUn1vB8EjsmLB5f69PbsDGuoNCbAUbyS3t6sEaWJM6B` | log `arras:rechazo:E53_TICKET_INVALIDO` (refunded by the token); `authorized`; `refunded`; `withdrawn` |
| 13. close | the human takes the rest of the budget back, revokes the agent and withdraws | `2eq7Gopj96uLn8ibGTKtMuYbT9PTzyVnQUpYvkTPmNoa`; `3ppCBse1WcRktZEUEjANevK38NSaNjaBAsKc2D7x392n`; `51DKxfopvSmCC1iwBFRUzuRASRbpBXB4U3M2VMJf9MXE` | `budget_withdrawn` 0.5; `agent_revoked`; `withdrawn` 0.5 |

Balances after: payer 15.3 (-1.7), worker 4.089 (+1.479), fee recipient 0.351 (+0.221), contract **0**; `get_config`:
`tvl_global` 0, `budgets_total` 0, `in_flight` 0. What went in through `ft_transfer_call` (2 + 0.5 + 0.1) equals what
went out (1.479 + 0.221 + 0.3 + 0.1 + 0.5).

### The mainnet script rehearsed: `v2.arras-v2e-1107.testnet` and `v2.arras-v2f-1107.testnet` (2026-11-07)

`scripts/mainnet-v2.sh`, the script that deployed v2 to mainnet on 2026-11-08 ([docs/EVIDENCE.md](docs/EVIDENCE.md),
section 6), ran end to end on testnet first over the same layout as mainnet (rebuilt first with `scripts/mainnet.sh`).
The first rehearsal (`v2.arras-v2e-1107.testnet`, with the script as it was before its own money review): the reviewed
commit `ddc5121`, wasm `1541525c...fa77`, deployed with a private `new` into the reserved `v2.` account, operator `em`
with its own capturer and arbiter (one function-call key each), a 7-day arbiter window and an operator ticket key, and
then every key of the contract deleted. Then its smoke
test, the security track: a human (the funding account) deposited a 0.10 budget for an agent, with the operator's
budget ticket; the agent, holding only a function-call key for `authorize_from_budget`, paid 0.05 to a worker with a
payment ticket; the capturer released it (0.0435 to the worker, 0.0065 to the fee recipient); the agent paid another
0.05, the human disputed it and the arbiter gave all of it back. The token was a test NEP-141
(`usdc-ensayo-1107.testnet`; Circle's testnet faucet sits behind a reCAPTCHA). A second rehearsal, after
the two rounds of that review (`v2.arras-v2f-1107.testnet`), recovered from transient failures by running each step
again, found the up-front reserve behind the script's last fix (what each call reserves up front) and finished with the
final script. Every transaction, the measurements and the budget for mainnet: [docs/MAINNET.md](docs/MAINNET.md),
section 8.

The checks below are on the first rehearsal's contract, except the last row, the second rehearsal's.

| Check | Command | Result |
|---|---|---|
| code = reproducible build, no keys | `near account view-account-summary v2.arras-v2e-1107.testnet network-config testnet now` | SHA-256 `1541525c...fa77`; no access keys |
| source commit (NEP-330) | `near contract call-function as-read-only v2.arras-v2e-1107.testnet contract_source_metadata json-args '{}' network-config testnet now` | `source_code_snapshot ...?rev=ddc51211ed93973ec9eb537a85bac39367f6e763` |
| the operator | `... get_operator json-args '{"operator_id":"em"}' ...` | capturer `em-capturer-v2.arras-v2e-1107.testnet`, arbiter `em-arbiter-v2.arras-v2e-1107.testnet`, `arbiter_window_ns` 604800000000000, a `ticket_pubkey` |
| the agent | `near account list-keys agent-v2.arras-v2e-1107.testnet network-config testnet now` | one key: only `authorize_from_budget` on `v2.arras-v2e-1107.testnet` |
| the two smoke payments | `... get_payment json-args '{"payment_id":"5AkgphxKw9ghWReKiJ5j1uUWE9ytPoxm1e6PGUKTzZHe"}' ...` and `HfTEaoU9bnXG6CpfxUWvhr1rV8RTEbU5v12ir6CsKwF6` | `Released`; `Resolved` with `refunded` 50000 |
| the second rehearsal, which finished with the final script: code = reproducible build, no keys | `near account view-account-summary v2.arras-v2f-1107.testnet network-config testnet now` | SHA-256 `1541525c...fa77`; no access keys (what its `verificar` read: [docs/MAINNET.md](docs/MAINNET.md), 8.6) |

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
| `contracts/escrow-v2` | the v2 contract (v1 + budgets, disputes, tickets); `src/unit_tests/`, `tests/workspaces.rs` (W2-*, U-24). It uses `escrow-core` without changing it: any change to the core changes v1's wasm even if v1 does not use it (measured), so v2's additions live in its own crate |
| `contracts/test-ft` | near-contract-standards fungible token, only for the sandbox tests |
| `contracts/registry-v1` | the agent registry (identity + reputation tied to Arras payments); `src/unit_tests/`, `tests/workspaces.rs` (WR-*) |
| `contracts/test-escrow` | a mock escrow whose `get_payment` returns raw JSON, only for the registry's sandbox tests |
| `contracts/erc8004-common`, `erc8004-identity`, `erc8004-reputation`, `erc8004-arras-verifier`, `erc8004-validation` | the ERC-8004 port (see "ERC-8004 on NEAR"); unit tests in each crate's `src/unit_tests/` (in `erc8004-common`, inside each module), the sandbox cycle across all of them in `contracts/erc8004-reputation/tests/workspaces.rs` (WE-*) |
| `contracts/test-nft-receiver` | a mock NFT receiver, only for the Identity Registry's sandbox tests |
| `docs/vectores.json` | golden vectors V-1..V-3 and F-1..F-7; section `v2`: v2 `msg`s, VB-1, tickets T-1/T-2 |
| `scripts/testnet.sh` | the v1 testnet sequence |
| `scripts/testnet-v2.sh` | the v2 testnet walkthrough |
| `scripts/mainnet.sh`, `scripts/test_mainnet.sh` | the mainnet deployment (dry run by default; the same script rehearsed on testnet) and its offline tests |
| `scripts/mainnet-v2.sh` | v2's mainnet deployment and its smoke test (dry run by default; rehearsed on testnet; run on mainnet on 2026-11-08) |
| `scripts/arras-comun.sh` | what both deployment scripts share (networks, flags, caps, transactions, RPC reads, the pinned wasm, keys through a pipe), written once |
| `scripts/test_cadena.py` | the fake chain of the offline tests of `scripts/mainnet-v2.sh` |
| `docs/MAINNET.md` | the mainnet runbook: pinned wasm, accounts and signers, the NEAR it needs (measured), the order with the facilitator, the testnet rehearsal, and the mainnet run of 2026-11-03; section 8, v2's runbook, its testnet rehearsals and its mainnet run of 2026-11-08 |
| `docs/EVIDENCE.md` | the mainnet transactions behind each claim (deployments, Execution Market's test payments and golden flows, v2's smoke test), with explorer links |
| `docs/plans/` | plans written before the code (the ERC-8004 port: `erc8004-near-port.md`) |
| `docs/handoffs/` | handoffs between workers (decisions and what is left) |
| `HACKATHON.md` | what existed before the hackathon and what was built during it |

Pinned: near-sdk and near-contract-standards `=5.29.1`, near-workspaces `=0.23.0`, cargo-near 0.22.0, near-cli-rs
0.30.1. `Cargo.lock` is committed (the reproducible build requires it).

## License

MIT. See [LICENSE](LICENSE).
