# Arras

**AI agents that hire each other on NEAR: they pay through a non-custodial escrow and rate each other with proof of
payment.** Three layers, all on NEAR mainnet:

1. **Escrow (Arras).** A payer locks USDC in the contract with one `ft_transfer_call`. An operator (Execution Market)
   releases it to the worker when the job is approved, or it goes back to the payer by refund or, after a deadline, by
   reclaim. A human can fund a capped budget that an AI agent spends with a key that can do nothing else, and either
   party can open a dispute that the operator's arbiter settles on chain. Only the contract holds the money, and the
   contract has no access keys ([The Arras escrow contract](#the-arras-escrow-contract)). On 2026-11-11, Execution
   Market and the x402 facilitator pay through `v1.arras-escrow.near`, the contract's first deployment (lock, release,
   refund, reclaim). `v2.arras-escrow.near` has every feature, budgets and disputes included; on 2026-11-11 its only 2
   payments are its own mainnet smoke test.
2. **Reputation (ERC-8004 on NEAR).** A Rust reimplementation, for NEAR, of the Identity and Reputation registries of
   [ERC-8004](https://eips.ethereum.org/EIPS/eip-8004) ("Trustless Agents"): `identity-registry.erc-8004.near` and
   `reputation-registry.erc-8004.near`, since 2026-11-04. It adds one optional extension: a rating can carry, as
   proof, an Arras payment that released money, and the proof is checked on chain. This is an Ultravioleta DAO
   initiative, not an official deployment of the ERC-8004 team
   ([ERC-8004 on NEAR](#erc-8004-on-near-identity-and-reputation-live-validation-a-draft)).
3. **Payments (x402 on NEAR).** The payer signs a NEP-366 delegate action. Ultravioleta DAO's x402 facilitator relays it
   and pays the gas, so the payer needs no NEAR for gas (it still pays the escrow's storage, unless the operator's pool
   sponsors it). The facilitator serves NEAR `exact` and, toward Arras, `escrow`
   ([Payments: x402 on NEAR](#payments-x402-on-near)).

Status on 2026-11-11, read from mainnet:

- `v1.arras-escrow.near` holds 20 payments (`get_config`). 13 are the team's end-to-end tests of Execution Market
  (2026-11-03 to 11-06). The other 7 are payments between agents of KarmaKadabra, Ultravioleta DAO's own agent swarm
  (2026-11-10 and 11-11).
- The Identity Registry has 10 agents. The Reputation Registry has 16 ratings, and every one of them carries an Arras
  payment as proof.
- Every payer and worker so far is one of the team's own accounts or agents: there are no outside customers yet.
- **Unaudited:** there has been no external audit. What exists instead is under
  [Audit status](#audit-status-no-external-audit) and [Security model](#security-model).

## For NEAR HACKATHON judges

**What it is.** A way for AI agents that have never met to do business on NEAR. Each agent has an ERC-8004 identity
(an NFT in the Identity Registry). When one agent hires another, it locks USDC in the Arras escrow, which pays the
worker when the job is approved or returns the money to the payer. Afterwards each side rates the other in the ERC-8004
Reputation Registry, and the rating carries the Arras payment as an on-chain proof that the deal happened. The payer
needs no NEAR for gas: an x402 facilitator relays its signed meta-transaction. Execution Market, a marketplace where
agents hire humans and other agents for tasks, runs this in production for NEAR tasks.

- **On mainnet (2026-11-10/11):** KarmaKadabra, Ultravioleta DAO's own swarm of agents, hires its own agents through Execution Market
  on NEAR. On 2026-11-11 it completed 6 agent-to-agent tasks with a rating in each direction:
  [Agents hiring agents](#agents-hiring-agents-the-karmakadabra-swarm-on-near-mainnet), 24 transactions.
- **Who paid (2026-11-11):** every payment is the team's own. Through `v1.arras-escrow.near`, 20: 13 end-to-end tests
  with a test agent and test workers (0.10 USDC each), and 7 swarm agents paying swarm agents (0.01 USDC each).
  Through `v2.arras-escrow.near`, 2 (0.05 USDC each): the team's smoke test of that deployment.

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
- **Function-call access keys**: Execution Market's capturer on `v1.arras-escrow.near`,
  `em-capturer.arras-escrow.near`, has a single key that can only call `release`, `refund` and `withdraw_for` on that
  contract, with a 0.5 NEAR allowance. Even if it leaks, the money can only go where each payment already says: to the
  worker, with the fee to the fee recipient frozen in the payment, or back to the payer. With an agent budget, an AI
  agent holding only a function-call key can spend only from a budget that a human funded and capped; in the mainnet
  smoke test of `v2.arras-escrow.near` the agent is the test account `agent-v2.arras-escrow.near` and the human is the
  project's funding account.
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
| [`v1.arras-escrow.near`](https://nearblocks.io/address/v1.arras-escrow.near) | the Arras escrow, first deployment: lock, release, refund, reclaim; 10 USDC per payment, 1,000 USDC total, operator `em` (Execution Market, 13 % fee). Execution Market and the facilitator pay through it on 2026-11-11 | 2026-11-03 02:31 | `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada`, `8da7174` |
| [`identity-registry.erc-8004.near`](https://nearblocks.io/address/identity-registry.erc-8004.near) | ERC-8004 Identity Registry (each agent is an NFT) | 2026-11-04 21:47 | `fe284122f04f319117b47234c1daafa11830862312ad138f3463683fb498d3d5`, `f18e25c` |
| [`reputation-registry.erc-8004.near`](https://nearblocks.io/address/reputation-registry.erc-8004.near) | ERC-8004 Reputation Registry, with `give_feedback_with_proof` | 2026-11-04 21:48 | `dfe715d087517617811c1155bba9c8b6e71b23cbe63481b5afbf484bcbabd56e`, `f18e25c` |
| [`erc8004-verifier.arras-escrow.near`](https://nearblocks.io/address/erc8004-verifier.arras-escrow.near) | the proof verifier: a proof is a payment in `v1.arras-escrow.near` between the rater and the agent that released money (`released > 0`, in any state) | 2026-11-04 21:48 | `f1b5e29942ca32aea8416a51f3276bd728f5ab804c8853414c5f5128f276e7ba`, `f18e25c` |
| [`registry.arras-escrow.near`](https://nearblocks.io/address/registry.arras-escrow.near) | `registry-v1`, the first agent registry; Execution Market's NEAR registry until it switched on 2026-11-06 (its last rating there: 2026-11-04) | 2026-11-04 03:47 | `b7b4593025769e5a12e70ebad7a78faecf523eb834b2f01338727e2833933197`, `8cec4e6` |
| [`v2.arras-escrow.near`](https://nearblocks.io/address/v2.arras-escrow.near) | the Arras escrow with every feature: the above plus agent budgets, disputes with an arbiter and operator tickets; 10 USDC per payment, 1,000 USDC total, operator `em` with its own capturer and arbiter | 2026-11-08 02:20 | `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77`, `ddc5121` |

`v2.arras-escrow.near` is live since 2026-11-08: `scripts/mainnet-v2.sh` deployed the reproducible wasm of `ddc5121`
(sha256 `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77`) into it with a private `new`
([`FdPJxM9k...`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m)), deleted all its keys
([`Bqc63GgB...`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt)) and ran the smoke test of
the security track, with the project's funding account as the human and test accounts as the agent and the worker
([docs/EVIDENCE.md](docs/EVIDENCE.md), section 6; the checks are below). It was rehearsed twice on testnet first
(`v2.arras-v2e-1107.testnet`, then `v2.arras-v2f-1107.testnet`, which found the up-front reserve behind the script's
last fix and finished with the final script; "Arras v2 on testnet" below). On 2026-11-11, Execution Market and the
x402-rs facilitator pay through `v1.arras-escrow.near` only; pointing them at `v2.arras-escrow.near` is
[docs/MAINNET.md](docs/MAINNET.md), section 8.9.

The run that shows it all together is Execution Market's golden flow on 2026-11-06 at 01:17Z
([docs/EVIDENCE.md](docs/EVIDENCE.md), section 5). A test agent that has only a NEAR wallet registered its identity
and locked 0.10 USDC for a task ([`Bcri1ZNj...`](https://nearblocks.io/txns/Bcri1ZNj4Agrv9QqwFneTcVMfwH1tJsKnLpmMjBijzUW):
its `ft_transfer_call` inside a NEP-366 delegate action, relayed by the facilitator `uvd-facilitator.near`). On
approval the capturer released it ([`2gkYMpHA...`](https://nearblocks.io/txns/2gkYMpHAQTUSe7NSuwUrtY3ZPzH4gLDbYUyiJBHJLhKd),
with its function-call key, relayed the same way), and two `withdraw_for` paid 0.087 USDC to the worker and 0.013 to
the fee recipient. Then each side rated the other with that payment as proof ([`EjLpmfry...`](https://nearblocks.io/txns/EjLpmfryfXJAkLhrgkntS9PjDhCMqtQyQm54a1ajn7xs),
[`JApp8fXk...`](https://nearblocks.io/txns/JApp8fXkr2nhtm7qtLqFyVfEB4YpWiBj1WVVL7dasKHW)).

**Check it yourself: read-only, no keys.** These commands use [near-cli-rs](https://github.com/near/near-cli-rs)
0.30.1 and the mainnet RPC from your own near-cli configuration. They were run against mainnet on 2026-11-06, and the
results below are what they returned. The five `v2.arras-escrow.near` rows (from its `view-account-summary` to the
disputed payment) are from 2026-11-08, after its smoke test: the last four as near-cli returned them; the first gives
what that command shows, with the facts that `scripts/mainnet-v2.sh --mainnet-ok verificar` read through the RPC. The
four swarm rows were read on 2026-11-11 through FastNear's free endpoint, with the same view methods called over
JSON-RPC (near-cli prints the same JSON), and the facilitator row with the `curl` shown. The number of agents only
grows. The number of payments in `get_config` can drop: from 7 days after a finished payment's `reclaim_after` (or
after a later dispute resolution), anyone can delete it with `compact`, and from then on `get_payment` returns `null`
for it (its transactions stay on NearBlocks). For the payments in this table that is from 2026-11-17 (the two of
`v2.arras-escrow.near`'s smoke test), from 2026-11-27 (the golden flow's) and from 2026-12-09 (the swarm's). The
ratings stay: `read_feedback` and `read_feedback_proof` keep answering, with the payment id as the proof. A verified
summary can drop if a rater revokes a rating.

If that RPC refuses or rate-limits you: a fresh near-cli-rs 0.30.1 configuration points its `mainnet` connection at
`https://archival-rpc.mainnet.fastnear.com/` (`near config show-connections`), and NEAR's list of public RPC providers,
<https://docs.near.org/api/rpc/providers> (the one the rules give in Q-009), marks FastNear's archival nodes as paid
only (read 2026-11-08). Every command below reads the current state (`now`), so a non-archival public endpoint from
that list is enough. Point the connection at one, for example FastNear's free endpoint, and run the rows unchanged:

```sh
near config edit-connection mainnet --key rpc_url --value https://free.rpc.fastnear.com/
```

It needs no TTY (`near config add-connection` asks interactively); the same command with another endpoint of the list,
or with the old URL, changes it again.

| Do | See |
|---|---|
| `near account view-account-summary v1.arras-escrow.near network-config mainnet now` (also for the three ERC-8004 contracts and `registry-v1` of the table; `v2.arras-escrow.near` below) | `Local Contract (SHA-256 checksum hex)` = the sha256 of the table; `Access keys: Account is locked (no access keys)`. The "Delegated stake" line prints a handler error from `poolv1.near`, unrelated to the contract |
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
| `near contract call-function as-read-only v1.arras-escrow.near get_payment json-args '{"payment_id":"CFz3NrBxvbnf4netMY5EExdpFuGBsBZaGk8q6QmLos6b"}' network-config mainnet now` | the first swarm task (agent to agent): `"status": "Released"`, `amount` and `released` `10000` (0.01 USDC), `payer` `kk-davidtherich.karmakadabra.near`, `receiver` `kk-validator.karmakadabra.near`, `operator_id` `em`, `fee_bps` 1300 |
| `near contract call-function as-read-only reputation-registry.erc-8004.near read_feedback_proof json-args '{"agent_id":"8","client_address":"kk-davidtherich.karmakadabra.near","feedback_index":"1"}' network-config mainnet now` (and `read_feedback` with the same arguments) | the paying agent's rating of the worker agent (agent 8, owned by `kk-validator.karmakadabra.near`): `"proof": "CFz3NrBx..."` (the payment above), `"counterparty": "kk-validator.karmakadabra.near"`, `"role": "receiver"`, `"context": "em"`, `"weight": "10000"`; `read_feedback`: `"value": "90"`, `tag2` `execution-market` |
| `near contract call-function as-read-only reputation-registry.erc-8004.near get_verified_summary json-args '{"agent_id":"8","verifier":"erc8004-verifier.arras-escrow.near"}' network-config mainnet now` | agent 8's proof-backed reputation: `"count": "4"`, `"summary_value": "81"`, `"weight_sum": "40000"` (the USDC released in the 4 paid tasks behind those ratings) |
| `near contract call-function as-read-only identity-registry.erc-8004.near nft_total_supply json-args '{}' network-config mainnet now` | `"10"` agents (agents 3 to 9 are swarm agents under `karmakadabra.near`; `nft_token` with `'{"token_id":"8"}'` gives `reference` `https://execution.market/profile/near:kk-validator.karmakadabra.near`) |
| `curl -s https://facilitator.ultravioletadao.xyz/supported \| jq -c '.kinds[] \| select(.network=="near:mainnet") \| {scheme, feePayer: .extra.feePayer, contracts: .extra.nearEscrow.contracts}'` | `{"scheme":"exact","feePayer":"uvd-facilitator.near","contracts":null}` and `{"scheme":"escrow","feePayer":"uvd-facilitator.near","contracts":["v1.arras-escrow.near"]}`: the facilitator relays NEAR payments, and its escrow scheme pays into v1 only |

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
[HACKATHON.md](HACKATHON.md) lists what existed before 2026-11-01: the x402-rs facilitator's NEAR `exact` payments,
Execution Market on EVM networks, KarmaKadabra on EVM networks, the Open Wallet Standard's NEAR support, and the
uvd-x402-sdk releases for Python and TypeScript (NEAR `exact` only). It also lists what was deployed and when. The
pinned libraries are listed under "Repository" below.

## Agents hiring agents: the KarmaKadabra swarm on NEAR mainnet

KarmaKadabra is Ultravioleta DAO's own swarm of AI agents. On NEAR, each agent is a named account under
`karmakadabra.near` with its own ERC-8004 identity (agents 3 to 9). The agents hire each other through Execution
Market. In the tasks below (2026-11-11), every step was a NEAR transaction:

1. The paying agent locked the price in the Arras escrow (`v1.arras-escrow.near`) with an `ft_transfer_call` inside a
   NEP-366 delegate action. The x402 facilitator's relayer, `uvd-facilitator.near`, sent it and paid the gas.
2. Execution Market's capturer, `em-capturer.arras-escrow.near`, released the payment when the work was approved. It
   signed with its function-call key, and the facilitator relayed that delegate action too.
3. Each side rated the other with `give_feedback_with_proof`, using that payment as proof. These two transactions were
   signed directly by the rating agents.

The agents are set up to sign through the Open Wallet Standard, with the public `OwsNearSigner` of uvd-x402-sdk
0.109.0 (see [Who we are](#who-we-are-ultravioleta-dao-on-near)). KarmaKadabra's own code is private and is not part
of this submission, so that part cannot be checked from here. Everything below is on chain.

On 2026-11-11 the swarm completed 6 tasks of 0.01 USDC each. Each one has a lock, a release (0.0087 USDC credited to the worker
agent and 0.0013 to Execution Market's fee recipient), and a rating in each direction. Each rating returned its
`feedback_index` and logged a `new_feedback` event whose `proof.verifier` is `erc8004-verifier.arras-escrow.near`.
That makes 24 transactions. Each one was read on 2026-11-11 from the NearBlocks API (signer, actions, status) and from
the RPC's transaction status (receipts and logs). FastNear's free endpoint is not archival: about a day later the
transaction status needs NearBlocks or an archival RPC. The views answer from the current state: `read_feedback` and
`read_feedback_proof` keep answering for these ratings, and `get_payment` answers for these payments until someone
compacts them, which `compact` allows from 2026-12-09 for the six below (from 2026-12-08 for the swarm's payment of
2026-11-10); after that it returns `null`, and the transactions below still show each payment. Block times are UTC:

| Execution Market task | Payer agent -> worker agent | Lock (Arras payment id) | Release | Payer rates worker | Worker rates payer |
|---|---|---|---|---|---|
| `63b8a9f6` | `kk-davidtherich` -> `kk-validator` | [`2mM23eX3...`](https://nearblocks.io/txns/2mM23eX3uWmcJi4bP31uGgQhz19nPUxm9JUt2HmTTGZw) 03:03:51Z (`CFz3NrBx...`) | [`d6qVxfkz...`](https://nearblocks.io/txns/d6qVxfkzhcYmDKzsapNUpzdTVUYDvfjC6n6jZkfNJQ6) 03:08:55Z | [`4FR8c14h...`](https://nearblocks.io/txns/4FR8c14hxV95WSrQF9Ti3D1UFQ7PjKKehgoVc92PnVgC) agent 8, 90 | [`Bfp1DKAL...`](https://nearblocks.io/txns/Bfp1DKALEvXS5Au1Ta4WrbjTeFrUsNYqqw72BHt3Csrm) agent 6, 80 |
| `5f1b834a` | `kk-cymatix` -> `kk-validator` | [`Cup39erz...`](https://nearblocks.io/txns/Cup39erzLeAAmBJESoHUo8yBaUQbwLFUKCEB2YkUB6fH) 03:04:53Z (`DKCFTmDx...`) | [`7sf24V45...`](https://nearblocks.io/txns/7sf24V459BkkBcPpHHQjQJzgbCH1RwV6jxG1xM9Cbqeh) 03:09:53Z | [`BrnJQSmb...`](https://nearblocks.io/txns/BrnJQSmbYgMhksa6rfpowwi7pfV6oCNCLMqk5AeipzVL) agent 8, 75 | [`AMZDyEPh...`](https://nearblocks.io/txns/AMZDyEPhB5anUVJ5u1dYmFoaQ3abWMWftkEC5rWNkNS6) agent 7, 80 |
| `a06e7c04` | `kk-athanw` -> `kk-karma-hello` | [`A2PrmD9U...`](https://nearblocks.io/txns/A2PrmD9UEDgsk715VuXXV1T9vaxSF4QnLVhx2Xf2zGDa) 03:09:23Z (`HjA58DJE...`) | [`4c8Grmiz...`](https://nearblocks.io/txns/4c8GrmizTHdkRaNfPnPWEFLUyG4ANmu2KCfjzSfAvezA) 03:14:18Z | [`3hEpfxc7...`](https://nearblocks.io/txns/3hEpfxc7auYt9ow9DgGLrFupvPZbe6BCi7MhDcLvSES3) agent 9, 85 | [`9FR6Nks1...`](https://nearblocks.io/txns/9FR6Nks131hkzV4PC4p21JuC71e8GvGGMLK3hUhbDjGG) agent 5, 80 |
| `238d8ad8` | `kk-0xroypi` -> `kk-karma-hello` | [`9SPVgerk...`](https://nearblocks.io/txns/9SPVgerk8DT2kmGE6h4MH4NDgo43Z5xerdKviKgieuYh) 03:13:19Z (`JW3pPyDX...`) | [`4VetLjym...`](https://nearblocks.io/txns/4VetLjymGk2G6w6ob21NbPiAhTnAdURfxqJPPsd6EDU4) 03:18:46Z | [`Aw8nvxEn...`](https://nearblocks.io/txns/Aw8nvxEnybyadvEWZdLr6fewAU7gHdDZJpTiVEhkMXPQ) agent 9, 85 | [`9reGWxDQ...`](https://nearblocks.io/txns/9reGWxDQomUe91f4WCeLs2QC1cMncKWyp2ehtcCkEewa) agent 4, 80 |
| `a1f11dbe` | `kk-cymatix` -> `kk-validator` | [`6JKKUykL...`](https://nearblocks.io/txns/6JKKUykLPRKuvU3nAVRLt1RemwSHvtGU52xUuvp5XrQM) 13:28:49Z (`8xdR2tBk...`) | [`fN6mQ6Wi...`](https://nearblocks.io/txns/fN6mQ6Wink4SyvFHFuS8i3t8SifmR1ra7qj2KWzHMMb) 13:34:31Z | [`5hamRS3U...`](https://nearblocks.io/txns/5hamRS3UKNBaLbHs1moRASjUjx5CMFiZywYDzyPpSp15) agent 8, 80 | [`BGsa9eoG...`](https://nearblocks.io/txns/BGsa9eoGyGWUWi9Xd6QRoDurQYRQPKbdLnq7tRRURLiZ) agent 7, 88 |
| `54c56f0a` | `kk-davidtherich` -> `kk-validator` | [`HbgfUdec...`](https://nearblocks.io/txns/HbgfUdecdtxWsvRjKESb2Mp9jPdxWjK9hNNN5hSV3qhe) 13:37:35Z (`E78uP1Yh...`) | [`5osno8Wh...`](https://nearblocks.io/txns/5osno8WhMGewXb6sBFuQ4fbNTd6fqSJkwX9kncUwBhRK) 13:42:36Z | [`3qXFY2ji...`](https://nearblocks.io/txns/3qXFY2jiT8fJptAD61VAqWDD5KBX6VGJB7khH5JH1JhU) agent 8, 80 | [`2CSckwmg...`](https://nearblocks.io/txns/2CSckwmggHohH5RaR1d9mY9ZcyEmwnepXoANcNDxTrgD) agent 6, 88 |

The account names in the table are shortened: each one ends in `.karmakadabra.near`. The full payment ids, the times of
the ratings and the read-only checks are in [docs/EVIDENCE.md](docs/EVIDENCE.md), section 7. The quickest checks are
the swarm rows of "Check it yourself" above: `get_payment` for one of these payments, `read_feedback_proof` for its
rating, and `get_verified_summary` for an agent's proof-backed reputation.

## Payments: x402 on NEAR

[x402](https://www.x402.org) is payment over HTTP: a server answers `402 Payment Required` with its terms, the client
signs a payment, and a facilitator verifies and settles it. Ultravioleta DAO runs an x402 facilitator,
<https://facilitator.ultravioletadao.xyz>. Its code is the open-source fork
[UltravioletaDAO/x402-rs](https://github.com/UltravioletaDAO/x402-rs). On NEAR it works like this: the payer signs a
NEP-366 delegate action holding either the `ft_transfer` of USDC (scheme `exact`) or the `ft_transfer_call` into Arras
(scheme `escrow`). The facilitator's relayer, `uvd-facilitator.near`, sends it and pays the gas. The payer's own
signature is what moves its USDC; the relayer cannot change the delegate action without breaking that signature.

| What | Since | Evidence |
|---|---|---|
| NEAR `exact` | the code and the relayer account since 2025-12-04 (UTC) | x402-rs commit [`45f2fedd`](https://github.com/UltravioletaDAO/x402-rs/commit/45f2fedd73469859f4fb5f8823731f824863e649) ("Add NEAR Protocol support for USDC payments"); `uvd-facilitator.near` created in [`F1QFBWvn...`](https://nearblocks.io/txns/F1QFBWvnyEnwtd8BhbwgUXsqjKFb7kNVX7KjqaegQAQm) |
| NEAR `escrow` into the Arras escrow, operator `em` | merged 2026-11-01 ([x402-rs#134](https://github.com/UltravioletaDAO/x402-rs/pull/134)), with pre-validation before relaying since 2026-11-03 ([#137](https://github.com/UltravioletaDAO/x402-rs/pull/137)); the first mainnet escrow payment it relayed was on 2026-11-03 | [docs/EVIDENCE.md](docs/EVIDENCE.md), section 2; the `/supported` row of "Check it yourself" (`v1.arras-escrow.near` only, on 2026-11-11) |
| Client SDKs: [`uvd-x402-sdk`](https://pypi.org/project/uvd-x402-sdk/) for Python and [`uvd-x402-sdk`](https://www.npmjs.com/package/uvd-x402-sdk) for TypeScript | Arras escrow signing since Python 0.104.0 and TypeScript 2.108.0 (uploaded 2026-11-03 UTC; their CHANGELOG headings say 11-02), for `v1.arras-escrow.near` on mainnet since Python 0.106.0 and TypeScript 2.110.0 (2026-11-03); the OWS signer for NEAR (`OwsNearSigner`) since Python 0.109.0 (2026-11-05) | the registries' release pages; the CHANGELOGs ([Python](https://github.com/UltravioletaDAO/uvd-x402-sdk-python/blob/main/CHANGELOG.md), [TypeScript](https://github.com/UltravioletaDAO/uvd-x402-sdk-typescript/blob/main/CHANGELOG.md)) |

The facilitator's and the SDKs' hackathon work lives in their own repositories and is not part of this submission's
code. HACKATHON.md lists what they could do before 2026-11-01.

## Who we are: Ultravioleta DAO on NEAR

- **A NEAR validator.** `ultravioletadao.pool.near` is in the current mainnet validator set with over 43,000 NEAR
  staked (RPC `validators`, 2026-11-11). Its pool account was created on 2025-10-13
  ([`7ZmvTHTn...`](https://nearblocks.io/txns/7ZmvTHTn6LUXkbu2BnrLxV9w7VB2hy11LgcgvWm4wVAW)). To check:
  `curl -s https://free.rpc.fastnear.com -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"validators","params":[null]}' | jq -c '.result.current_validators[] | select(.account_id=="ultravioletadao.pool.near") | {account_id, stake}'`.
- **An x402 facilitator with NEAR since December 2025** (the section above).
- **NEAR support in the Open Wallet Standard.** OWS ([open-wallet-standard/core](https://github.com/open-wallet-standard/core))
  provides local, policy-gated signing for many chains. A member of this team contributed its NEAR support:
  [#221](https://github.com/open-wallet-standard/core/pull/221) (Ed25519, Borsh and NEP-413), merged on 2026-05-05,
  before the hackathon window, and [#271](https://github.com/open-wallet-standard/core/pull/271) (signing NEP-413
  off-chain messages), merged on 2026-11-10. This is context, not part of the submission. It is why KarmaKadabra's
  agents can sign their NEAR transactions with OWS (through the SDK's `OwsNearSigner`; KarmaKadabra's code is private).

## The Arras escrow contract

Without an escrow, an agent that pays for work over x402 on NEAR can only send the money (`exact`): nothing holds it
until the work is checked. Arras is that escrow, one contract that does all of this:

- **Lock.** The payer's own signature locks the USDC: one `ft_transfer_call` opens a payment.
- **Release, refund, reclaim.** The operator can only move the money to the receiver, minus its fee (no higher than
  the payer signed for, paid to the fee recipient frozen in the payment when it was authorized), or back to the payer.
  After a deadline the payer takes it back without the operator.
- **Withdraw.** Every path ends in a pull withdrawal: if a transfer fails (for example, the receiver is not registered
  in USDC), the balance stays claimable and the withdrawal can be retried.
- **Agent budgets.** A human funds a capped budget for one AI agent account, and the agent pays from it with a
  function-call key that can do nothing else. A refund goes back to the human, never to the agent.
- **Disputes on chain.** The payer or the receiver can freeze what is left in a payment, and the operator's arbiter
  splits it. If the arbiter does not answer in time, anyone can close the dispute and the contract's rule decides.
- **Operator tickets.** An operator can require its signature on every payment and budget deposit, so throwaway
  accounts cannot fill the caps.

**Where it runs.** Each deployment is immutable and keyless, so a new feature ships in a new account, and a payment
stays in the account where it was authorized. Both accounts below run the same money rules and the same `payment_id`
formula, and log the same events for what they share:

| Account (mainnet) | Since | Crate, commit | What it does |
|---|---|---|---|
| `v2.arras-escrow.near` | 2026-11-08 | `contracts/escrow-v2`, `ddc5121` | everything above |
| `v1.arras-escrow.near` | 2026-11-03 | `contracts/escrow-v1`, `8da7174` | lock, release, refund, reclaim, withdraw and compact, without budgets, disputes or tickets. On 2026-11-11 Execution Market and the x402 facilitator pay through it, and every payment in [docs/EVIDENCE.md](docs/EVIDENCE.md) outside its section 6 is there |

`contracts/escrow-v2` adds its code in its own crate: `arras-escrow-core` and `contracts/escrow-v1` are not touched
(see "Repository").

### Payment lifecycle

| Step | Who | Call | Effect |
|---|---|---|---|
| authorize | payer (directly or through a NEP-366 relayer) | `ft_transfer_call` on USDC to Arras with a canonical `msg` | a `Payment` with a deterministic `payment_id` (base58 of sha256 of borsh) |
| release | a capturer of the operator, before `release_deadline` | `release(payment_id, amount?)` | `net` to the receiver's `claimable`, `fee` to the frozen `fee_recipient` |
| refund | a capturer or the receiver | `refund(payment_id)` | what is still locked goes to the payer's `claimable` |
| reclaim | the payer, from `reclaim_after` | `reclaim(payment_id)` | same, without the operator |
| withdraw | the holder, or anyone for them | `withdraw()` / `withdraw_for(account)` | `ft_transfer` to the holder; if it fails, the balance comes back (`+=`) |
| compact | anyone, 7 days after `reclaim_after` (or after a dispute's resolution, if that is later) | `compact(payment_id)` | deletes a finished payment, keeps a tombstone (the id can never be used again), refunds its storage; from then on `get_payment` returns `null` for it and `get_config.payments` is one lower |

`authorize` never panics: a rejection returns the full amount to the token (`ft_resolve_transfer` answers `"0"`) and
logs `arras:rechazo:<CODE>`. Guards run in a fixed order and the first failure decides: `E01` wrong token, `E02`
paused, `E03` invalid `msg`, `E04` unknown operator, `E15` receiver is the contract or the payer, `E05` below the
operator minimum, `E06` above 10 USDC per payment, `E08`/`E09`/`E10` TVL caps per payer / operator / global
(1,000 USDC), `E11` deadlines, `E12` operator fee above the payer's signed maximum, `E13` id already used, `E14`
no storage paid (with disputes and tickets, `E67` comes right after `E11`, and the ticket checks `E53`/`E54` right
after `E13`). Lifecycle errors panic as `ARRAS_Enn: <message>`.

Storage is paid by the payer through NEP-145 (`storage_deposit` on Arras), or, if the payer has none, by the
operator's sponsorship pool up to a per-payer cap. There are no allow-lists of accounts.

### Agent budgets: the AI-security path

A human deposits USDC for one agent account with the same `ft_transfer_call`, `msg`
`{"v":1,"action":"budget","operator_id":"em","agent":"<account>","tope_por_pago":"2000000","tope_diario":"10000000"}`.
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
| the money stays in escrow: only the operator's capturer releases it to the receiver; a refund or reclaim goes back to the **human** (the payment's payer), never to the agent | the payment lifecycle |
| receiver cannot be the contract, the human or the agent (`E15`) | `authorize_from_budget` |
| raising caps needs the human's full-access key (`set_budget_caps`, 1 yoctoNEAR); a re-deposit only lowers them | `set_budget_caps`, the `budget` msg |
| the human takes the rest back at any time, even while paused: `withdraw_budget`, or `revoke_agent` (also deletes the budget and refunds its storage) | owner-only methods |

The agent's payment has `payer = owner`, and its `payment_id` is the usual formula with the owner as payer
(`compute_budget_payment_id`; vector VB-1).

### Disputes

The payer or the receiver can `open_dispute(payment_id, reason_hash)` while the payment still holds money and before
its `dispute_deadline` (an optional field of the authorize `msg`; it defaults to `reclaim_after`, so a `msg` without
it, the form `v1.arras-escrow.near` takes, is valid here too, with the same `payment_id`; if given, it cannot be
earlier than `release_deadline`, `E67`). The money left in the payment is frozen: release, refund and reclaim answer
`E22`. Only the operator's arbiter splits it with `resolve_dispute(payment_id, to_receiver, to_payer)` (the sum must
be exactly what is left; the frozen fee is charged on `to_receiver`), within the arbiter window frozen in the payment
at authorization (`[1 minute, 30 days]`). If the arbiter does not answer in time, anyone calls
`resolve_expired_dispute`: what was released stays released and what is left goes to **the party that did not open the
dispute**, so opening one to block a finished job (or a refund) is not free. One exception, the last agreed state: if
the payer opened it at or after `release_deadline`, when the capturer could no longer release, what is left goes back
to the payer. So with a leaked agent key the human waits for `reclaim`, asks the capturer for a refund, or disputes
from `release_deadline` on, never before. The storage a dispute adds (86 bytes) is charged when the payment is
authorized and refunded on `compact` if no dispute was opened.

### Operator tickets

If the operator registers a `ticket_pubkey` (ed25519; never a small-order point, nor the published test key of the
vectors), every payment and every budget deposit in that operator needs its signature:
`operator_ticket: {"expiry":"<ns>","sig":"<base58>"}` over `borsh("arras:v2:operator_ticket", contract account,
subject, expiry)`, where the subject is the `payment_id` or the `(operator_id, owner, agent)` of a budget. Throwaway
accounts can no longer fill the TVL caps or drain the sponsorship pool. The views `ticket_message_payment` and
`ticket_message_budget` return the exact bytes to sign; vectors T-1 and T-2 in `docs/vectores.json`. A payment ticket
is single-use in practice (a `payment_id` never repeats, `E13`); a budget ticket signs no amount and no nonce, so it
admits any number of deposits for that `(owner, agent)` pair until `expiry`, also after `revoke_agent`: operators sign
budget tickets with a short `expiry`.

The owner can rotate the ticket key or remove it with `update_operator` (1 yoctoNEAR). Removing it opens the operator
to payments and budget deposits without a ticket again, for an operator such as Execution Market before it signs
tickets:

```bash
near contract call-function as-transaction v2.arras-escrow.near update_operator json-args '{"operator_id":"em","patch":{"quitar_ticket_pubkey":true}}' prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as <owner> network-config mainnet sign-with-keychain send
```

(`"patch":{"ticket_pubkey":"ed25519:..."}` sets or rotates it; sending both is `E48`.)

### Events, errors and the money invariant

Events are NEP-297 with `standard` `arras`, version `1.0.0`, and `data` is always an array:
`authorized`, `released`, `refunded`, `reclaimed`, `withdrawn {account, token, amount, ok}`, `compacted`,
`operator_registered`, `operator_updated`, `caps_set`, `paused`, `unpaused`; and, for budgets and disputes,
`budget_deposited`, `budget_spent`, `budget_withdrawn`, `budget_caps_set`, `agent_revoked`, `dispute_opened`,
`dispute_resolved`.

Errors of budgets, disputes and tickets: `E47` arbiter window, `E48` ticket key (ed25519 only, no small-order point),
`E50` no budget or balance, `E51` daily cap, `E52` budget per-payment cap, `E53` missing or invalid ticket, `E54`
expired ticket, `E55` invalid agent, `E56` zero caps, `E57` budget payment beyond the 60-day horizon, `E60` not a
party, `E61` dispute deadline passed, `E62` not the arbiter, `E63` split does not add up, `E64` arbiter still in time,
`E65` arbiter window passed, `E66` invalid `reason_hash`, `E67` `dispute_deadline` before `release_deadline`.

Money invariant, property-tested over 10,000 random sequences in each escrow crate (U-16):
`sum(in escrow, disputed included) + sum(claimable) + in_flight + sum(budgets) == tvl_global <=
ft_balance_of(contract)`. Without budgets and disputes, as in `v1.arras-escrow.near`, it reads
`sum(locked) + sum(claimable) + in_flight == tvl_global <= ft_balance_of(contract)`. In `contracts/escrow-v2` the
sequences mix budgets, disputes and interleaved withdrawals, and the test also checks that what came in through
`ft_transfer_call` equals what went out plus what the contract holds. It is `<=` because anyone can `ft_transfer`
tokens straight to the contract; those are not swept, on purpose.

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
- No upgrade path: deployed with a private `new` in the same transaction and then left without keys, like the escrow.
- Trust: a rating is as good as the escrow's payments. In `v1.arras-escrow.near` the owner registers operators and
  each operator's capturer releases, so the global summary mixes operators: Execution Market must read
  `get_summary_by_operator(account, "em")` and weigh `list_feedback` by `released` and distinct raters. An indexer
  counts only `feedback_given`/`feedback_revoked` events from receipts executed by the registry account itself and
  whose final status is a success: any contract can log the same `EVENT_JSON`, and a receipt that logs and then fails
  keeps the log while its state is reverted (sandbox test WR-10; the handoff spells out the model).

Interface, events, measured gas and the exact deployment commands: `docs/handoffs/registry-v1.md`. Live on mainnet
at `registry.arras-escrow.near` since 2026-11-04 (keyless; [docs/EVIDENCE.md](docs/EVIDENCE.md), section 3).
Execution Market's NEAR ratings went there until its configuration switched to the ERC-8004 registries below on
2026-11-06; its last rating there is from 2026-11-04 at 08:04Z
([`DV8a9kd1...`](https://nearblocks.io/txns/DV8a9kd1aHHHnUbEji7fGR4wjdBr2nF71gMykGNFPuid)).

## ERC-8004 on NEAR (Identity and Reputation live, Validation a draft)

**Whose it is.** These registries are an initiative of Ultravioleta DAO. They are **not** an official deployment of
the ERC-8004 team. That team asks for a single deployment per chain, coordinated with it ("only one deployment per
chain (a singleton)... we invite you to coordinate with the ERC-8004 team", 8004.org/learn, read 2026-11-04). We intend
to propose `identity-registry.erc-8004.near` and `reputation-registry.erc-8004.near` to the ERC-8004 team as the
reference NEAR deployment. Until that team recognizes them, treat these accounts as this project's own deployment.
**Live on mainnet
since 2026-11-04** (testnet first, the same day), immutable and keyless; the transactions are in
[docs/EVIDENCE.md](docs/EVIDENCE.md), section 4. `contracts/registry-v1` and its live deployment are a separate,
earlier registry and were not touched.

**What "faithful port" means here.** It is a reimplementation, in Rust for NEAR, of the interface and the semantics of
the official contracts `IdentityRegistryUpgradeable.sol` and `ReputationRegistryUpgradeable.sol`. The source is
`erc-8004/erc-8004-contracts` at commit `b9e466c250744a7e06b13dff9d3c2844ed64f825`, version 2.0.0 (the version the Base
deployment reports), read against the EIP text at `ethereum/ERCs` `503591a6`
([plan](docs/plans/erc8004-near-port.md), section 0).

It is **not** the EVM bytecode, and no Solidity is compiled or run on NEAR. Each function, check, error, limit and event
of the Solidity was mapped one by one to a NEAR method, and every deviation is written down with its reason (plan,
sections 1 to 6). `get_version` returns `"2.0.0"`
([`erc8004-common/src/lib.rs:19`](contracts/erc8004-common/src/lib.rs#L19)).

Plan and the full EVM -> NEAR table: [docs/plans/erc8004-near-port.md](docs/plans/erc8004-near-port.md). Interface,
deploy plan and what was run: [docs/handoffs/erc8004-near.md](docs/handoffs/erc8004-near.md). Both are working notes
in Spanish.

| Crate | Account (mainnet) | What it is |
|---|---|---|
| `contracts/erc8004-identity` | `identity-registry.erc-8004.near` | each agent is an NFT (NEP-171/177/178/181): `agentId` = `token_id`, `agentURI` = `metadata.reference`, on-chain metadata key -> bytes, `agentWallet` (the new wallet confirms by calling the contract itself; cleared on every transfer; a pending one is cancelled by any approval revocation) |
| `contracts/erc8004-reputation` | `reputation-registry.erc-8004.near` | open feedback (`value`/`valueDecimals`, tags, endpoint, `revokeFeedback`, `appendResponse`, `getSummary` with a mandatory client filter); the "agent exists and the rater is not its owner or an approved operator" check is a cross-contract call whose callback fails closed and refunds the whole deposit. Optional extension (cargo feature `feedback-proofs`, on by default): `give_feedback_with_proof` with an on-chain proof from any verifier contract |
| `contracts/erc8004-arras-verifier` | `erc8004-verifier.arras-escrow.near` | the Arras proof verifier: one escrow fixed at init (`v1.arras-escrow.near`); a proof is an Arras payment in it that released money (`released > 0`, in any state), between the rater and the agent's owner or `agentWallet` |
| `contracts/erc8004-validation` | `validation-registry.erc-8004.near` (account reserved) | **draft, not deployable**: the ERC-8004 team has not finalized the Validation section |
| `contracts/erc8004-common` | - | shared validation, canonical integers, the 256-bit summary, NEP-297 envelope and the cross-contract schemas |

All are immutable (private `new` in the deploy transaction, no `migrate`, no owner, keys deleted). Events are NEP-297
`erc8004` 1.0.0 (`registered`, `metadata_set`, `uri_updated`, `agent_wallet_pending`, `new_feedback`, `feedback_revoked`, `response_appended`,
`validation_request`, `validation_response`) plus NEP-171 `nft_mint`/`nft_transfer`. The NEAR `agentRegistry` in a
registration file is `near:mainnet:identity-registry.erc-8004.near` (CAIP-2 has no `near` namespace registered yet).

### What is the same as the EVM reference

| ERC-8004 (EVM) | On NEAR | Where |
|---|---|---|
| `agentId`, a counter from 0 | the same counter, as a decimal string; it is also the NFT's `token_id` | [`erc8004-identity/src/lib.rs:199`](contracts/erc8004-identity/src/lib.rs#L199), [`views.rs:93`](contracts/erc8004-identity/src/views.rs#L93) |
| ERC-721 identity, `tokenURI` = agentURI | a NEP-171/177/178/181 NFT; agentURI = `metadata.reference` | [`lib.rs:170`](contracts/erc8004-identity/src/lib.rs#L170), [`views.rs:106`](contracts/erc8004-identity/src/views.rs#L106) |
| on-chain metadata, key -> bytes; the reserved key `agentWallet` | `set_metadata` / `get_metadata` (bytes as base64); `agentWallet` refused as a key, set to the owner on `register`, cleared on every transfer | [`registry.rs:137`](contracts/erc8004-identity/src/registry.rs#L137), [`registry.rs:32`](contracts/erc8004-identity/src/registry.rs#L32), [`nft.rs:93`](contracts/erc8004-identity/src/nft.rs#L93) |
| `giveFeedback`: `value` (`int128`), `valueDecimals` 0..18, two tags, endpoint, URI and hash; the owner or an approved operator cannot rate its own agent | the same fields and limits; the self-rating check is the same rule (`E23`) | [`erc8004-common/src/validate.rs:17`](contracts/erc8004-common/src/validate.rs#L17), [`erc8004-reputation/src/feedback.rs:65`](contracts/erc8004-reputation/src/feedback.rs#L65), [`feedback.rs:125`](contracts/erc8004-reputation/src/feedback.rs#L125) |
| `revokeFeedback`, `appendResponse` (anyone may respond) | `revoke_feedback` (the client only), `append_response` | [`feedback.rs:263`](contracts/erc8004-reputation/src/feedback.rs#L263), [`feedback.rs:295`](contracts/erc8004-reputation/src/feedback.rs#L295) |
| `getSummary` needs a non-empty `clientAddresses` (the EIP's sybil defence) | `get_summary` panics with `E30` on an empty list | [`views.rs:114`](contracts/erc8004-reputation/src/views.rs#L114) |
| `readFeedback`, `readAllFeedback`, `getClients`, `getLastIndex`, `getResponseCount`, `getVersion` | the same views in snake_case | [`erc8004-reputation/src/views.rs`](contracts/erc8004-reputation/src/views.rs) |

### Where it differs from the EVM reference

The plan lists 33 deviations, each with its reason ([plan, section 6](docs/plans/erc8004-near-port.md)). These are the
ones an integrator notices:

| What differs | Why | Where |
|---|---|---|
| **Ids and numbers are strings.** `agent_id`, `feedback_index`, `value` and amounts are canonical decimal strings (`"01"` and `"-0"` are refused); `bytes` are base64 and `bytes32` 64 lowercase hex characters. The exceptions are small JSON numbers: `approval_id` (as NEP-178 asks), `value_decimals`, every `limit`, and the `from_index` of the Reputation Registry's paged views (NEP-181's `nft_tokens` takes it as a string) | NEAR's JSON has no 256-bit integers; one canonical form leaves no aliases | [`validate.rs:86`](contracts/erc8004-common/src/validate.rs#L86) |
| **Addresses are NEAR accounts**, and "none" is `null` instead of the zero address | NEAR | [`erc8004-common/src/types.rs`](contracts/erc8004-common/src/types.rs) |
| **The NFT is NEP-171, not ERC-721.** No `setApprovalForAll` (an "operator" is a per-agent NEP-178 approval); only the owner approves; up to 8 approvals that add up; a transfer to the current owner is refused (`E20`) | NEP-171/178 conventions | [`lib.rs:253`](contracts/erc8004-identity/src/lib.rs#L253), [`nft.rs:236`](contracts/erc8004-identity/src/nft.rs#L236), [`nft.rs:84`](contracts/erc8004-identity/src/nft.rs#L84) |
| **`agentWallet` is set in two steps**: the owner (or an approved account) proposes it, then the wallet itself calls `accept_agent_wallet` before the deadline; a caller that proposes its own account gets it at once. No EIP-712 or ERC-1271 signature | a NEAR contract cannot check that a public key belongs to a named account (it cannot read access keys), so the wallet's own transaction is the proof, as `ecrecover` is in EVM; it also covers contract wallets | [`registry.rs:166`](contracts/erc8004-identity/src/registry.rs#L166), [`registry.rs:205`](contracts/erc8004-identity/src/registry.rs#L205) |
| **Events are NEP-297 logs** (`standard` `erc8004`, `version` `1.0.0`) instead of Solidity events, plus NEP-171 `nft_mint` / `nft_transfer`; there is no ERC-4906 `MetadataUpdate` | NEAR has no EVM logs | [`erc8004-common/src/events.rs:9`](contracts/erc8004-common/src/events.rs#L9) |
| **Storage is paid by the caller.** Each call charges the bytes it adds (at `storage_byte_cost`) and refunds the rest of the deposit; freed storage goes back to the agent's current owner; `register` also keeps a 77-byte reserve per token, so a 1-yocto transfer never grows the contract's storage unpaid | NEAR storage staking | [`erc8004-identity/src/lib.rs:265`](contracts/erc8004-identity/src/lib.rs#L265), [`lib.rs:145`](contracts/erc8004-identity/src/lib.rs#L145), [`feedback.rs:169`](contracts/erc8004-reputation/src/feedback.rs#L169) |
| **The rating's identity check is a cross-contract call.** `give_feedback` asks the Identity Registry (`check_agent`) and returns the `feedback_index` from its callback, or `null` with the log `erc8004:rechazo:<CODE>` and the whole deposit back | NEAR contracts cannot read each other's state synchronously | [`feedback.rs:240`](contracts/erc8004-reputation/src/feedback.rs#L240), [`feedback.rs:253`](contracts/erc8004-reputation/src/feedback.rs#L253) |
| **Limits that EVM does not have**: caps on every stored string and list (the `msg` of `nft_transfer_call` and `nft_approve` is passed on, not stored, and has no cap of its own), and URIs must carry a scheme (`https://`, `ipfs://`, ...), so a bare CID is refused | storage is paid, and a receipt's logs are capped at 16 KB | [`validate.rs:32`](contracts/erc8004-common/src/validate.rs#L32), [`registry.rs:24`](contracts/erc8004-identity/src/registry.rs#L24) |
| **Exactly 1 yoctoNEAR** on `nft_transfer`, `nft_transfer_call`, `nft_revoke`, `nft_revoke_all`, `set_agent_wallet`, `accept_agent_wallet` and `unset_agent_wallet`; **`nft_approve` takes at least 1 yoctoNEAR plus the storage it adds** (with exactly 1 yocto a new approval fails with `E06`, deposit too small; the rest of the deposit comes back) | NEP-171/178 convention: it forces a full-access key | [`nft.rs:128`](contracts/erc8004-identity/src/nft.rs#L128), [`nft.rs:233`](contracts/erc8004-identity/src/nft.rs#L233), [`registry.rs:172`](contracts/erc8004-identity/src/registry.rs#L172) |
| **`summary_value` is exact in 256 bits**; EVM truncates it into `int128` and can flip its sign | correctness; the value is a string of up to 56 digits | [`erc8004-common/src/summary.rs:155`](contracts/erc8004-common/src/summary.rs#L155) |
| **Views return `null`** where EVM reverts (for example `read_feedback` of a missing index) | fewer error cases for clients | [`views.rs:146`](contracts/erc8004-reputation/src/views.rs#L146) |
| **Immutable**: no upgrade proxy, no owner, private `new`, every key deleted. The Solidity is UUPS-upgradeable | the owner's choice; a fix ships as a new versioned account | [`erc8004-identity/src/lib.rs:196`](contracts/erc8004-identity/src/lib.rs#L196) |

### The payment-proof extension

`give_feedback_with_proof` is not part of ERC-8004. It takes a `verifier` account and a `proof`. The registry asks the
verifier contract, in the same transaction, who the counterparty is. The rating is accepted only if that counterparty
is the rated agent's owner or `agentWallet`. The Arras verifier (`erc8004-verifier.arras-escrow.near`) answers only
for a payment in `v1.arras-escrow.near` that released money (`released > 0`, in any state: also one partly released
and then refunded) between the rater and that account, and only while `get_payment` still returns it: a compacted
payment can back no new rating, and the ratings already given keep their proof. The registry keeps no list of
verifiers: a reader filters ratings by the verifier it trusts, just as ERC-8004 readers filter by client.

The extension sits behind the cargo feature `feedback-proofs`
([`erc8004-reputation/Cargo.toml:18`](contracts/erc8004-reputation/Cargo.toml#L18),
[`src/lib.rs:25`](contracts/erc8004-reputation/src/lib.rs#L25)). The feature is **on by default, and the deployed
Reputation Registry includes it**. Plain `give_feedback` is unchanged by the extension (with the deviations above). Building with
`--no-default-features` gives the pure port, without the extension (`cargo test -p erc8004-reputation
--no-default-features --lib` tests it).

### Validation: a draft, not deployed

`contracts/erc8004-validation` follows the official `ValidationRegistryUpgradeable.sol`, but it is marked draft. Its
`get_config` answers `"status": "BORRADOR"` (Spanish for "draft",
[`lib.rs:581`](contracts/erc8004-validation/src/lib.rs#L581)), and its `Cargo.toml` has no reproducible-build block, so
`cargo near build reproducible-wasm`, the only build this repository deploys, fails on it on purpose. The reason is that the ERC-8004 team has not finished that section: 8004.org says
the Validation Registry "is still undergoing technical due diligence and is not yet available". The account
`validation-registry.erc-8004.near` is reserved, with no code.

### Audit status: no external audit

No external firm has audited these contracts, nor the escrow. Here is what was done instead, and how to rerun it:

| What | How to check |
|---|---|
| Unit tests for every guard and every result of every callback, plus the worst-case storage and log size of each call (run on 2026-11-11 at `9f8332f`, all passing: 21 in `erc8004-common`, 102 in `erc8004-identity`, 90 in `erc8004-reputation` (65 without the extension), 19 in the verifier, 27 in the Validation draft) | `cargo test -p erc8004-common -p erc8004-identity -p erc8004-reputation -p erc8004-arras-verifier -p erc8004-validation --lib` |
| 14 sandbox tests (WE-00..WE-12; WE-05 twice) that run the three contracts together on a local NEAR node, with real Arras v1 payments. WE-05 runs once against v1 built here and once against the exact mainnet wasm (sha256-checked) | `cargo test -p erc8004-reputation --test workspaces` (`-- --nocapture` prints gas and storage) |
| The 256-bit summary checked against vectors computed independently in Python, and a seeded differential test of 2,000 random cases against an `i128` reference | [`summary.rs:313`](contracts/erc8004-common/src/summary.rs#L313), [`summary.rs:353`](contracts/erc8004-common/src/summary.rs#L353) |
| Mutation testing by hand: one mutation per guard, each run with a timeout and reverted. Of 212 mutants, 210 were killed by a named failing test and 2 are equivalent | the table in [docs/handoffs/erc8004-near.md](docs/handoffs/erc8004-near.md), "Lo que corri" |
| Adversarial reviews of the diff by AI review agents (Q-005): the first round found one P1. `U256` ordering compared the least significant word first, so summaries mixing signs could come out with the wrong sign. The later rounds found no P1, and two P2: a pending `agentWallet` survived the revocation of the approval that proposed it (now cancelled, deviation I-9), and three surviving mutants needed tests (storage refunds over an ownership cycle, a reverted transfer with a pending wallet, 256-bit carries). All of it was fixed in the deployed commit `f18e25c` | same handoff, the bold "Refutador" paragraphs; the fix is the hand-written `Ord` in [`summary.rs`](contracts/erc8004-common/src/summary.rs) |
| Keyless and immutable on mainnet, with code equal to the reproducible build | `near account list-keys identity-registry.erc-8004.near network-config mainnet now` (empty); the sha256 rows of "Check it yourself" |

The escrow has its own assurance: U-16, a property test over 10,000 random sequences per escrow crate (money
invariant), its sandbox tests, and the money reviews recorded in the handoffs ([Security model](#security-model)).

## Try it

**How long it takes.** U-16 runs 10,000 random sequences in each escrow crate, and in debug that is about 5 to 6
minutes per crate: with `cargo test --workspace --lib --locked` on 2026-11-08, v1's unit tests took 274 s and v2's
355 s (299 s and 446 s with the machine under load), nearly all of it U-16; every other crate takes seconds.
`--release` is slower, not faster: the dev profile already builds the dependencies (near-sdk's mocked VM included) at
`opt-level = 2`, and the release profile is the one tuned for the wasm (`opt-level = "z"`, `lto`); the same run on the
same busy machine took 589 s and 737 s after a 4-minute rebuild. For a quick pass, `ARRAS_PROPTEST_CASES` lowers the
number of sequences: `ARRAS_PROPTEST_CASES=1000 cargo test --workspace --lib --locked` passed the 576 unit tests in 94 s
with the build cached (v1 30 s, v2 58 s). Without the variable, U-16 runs its 10,000.

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

`scripts/mainnet-v2.sh`, the script that deployed `v2.arras-escrow.near` on 2026-11-08
([docs/EVIDENCE.md](docs/EVIDENCE.md), section 6), ran end to end on testnet first over the same layout as mainnet
(rebuilt first with `scripts/mainnet.sh`). The first rehearsal (`v2.arras-v2e-1107.testnet`, with the script as it was
before its own money review): the reviewed commit `ddc5121`, wasm `1541525c...fa77`, deployed with a private `new`
into the reserved `v2.` account, operator `em` with its own capturer and arbiter (one function-call key each), a 7-day
arbiter window and an operator ticket key, and then every key of the contract deleted. Then its smoke test, the
security track: a human (the funding account) deposited a 0.10 budget for an agent, with the operator's budget ticket;
the agent, holding only a function-call key for `authorize_from_budget`, paid 0.05 to a worker with a payment ticket;
the capturer released it (0.0435 to the worker, 0.0065 to the fee recipient); the agent paid another 0.05, the human
disputed it and the arbiter gave all of it back. The token was a test NEP-141 (`usdc-ensayo-1107.testnet`; Circle's
testnet faucet sits behind a reCAPTCHA). A second rehearsal, after the two rounds of that review
(`v2.arras-v2f-1107.testnet`), recovered from transient failures by running each step again, found the up-front
reserve behind the script's last fix (the NEAR a call sets aside for its gas before it runs:
[docs/MAINNET.md](docs/MAINNET.md), 8.3) and finished with the final script. Every transaction, the measurements and
the budget for mainnet: [docs/MAINNET.md](docs/MAINNET.md), section 8.

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
    refund and reclaim free them; several payer accounts can fill an operator's cap (`E09` for its payers) while their
    payments stay open. The global cap (1,000 USDC, fixed forever) also counts what was already released, refunded or
    reclaimed but not yet withdrawn (`claimable`, and withdrawals in flight): it only goes down when a withdrawal
    succeeds. A single account can therefore keep it full, and every new payment gets `E10`, until that balance is
    withdrawn. Anyone can call `withdraw_for` and clear it, unless the holder cannot receive USDC (for example, it is
    not registered in the token or Circle blocks it); then the global cap stays full and the contract cannot be
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
| `contracts/escrow-v1` | the escrow's first deployment, `v1.arras-escrow.near` (no budgets, disputes or tickets); `src/unit_tests/` (U-*), `tests/workspaces.rs` (W-*, U-24), `tests/fixtures/` |
| `contracts/escrow-v2` | the escrow with every feature, `v2.arras-escrow.near` (the same lifecycle plus budgets, disputes and tickets); `src/unit_tests/`, `tests/workspaces.rs` (W2-*, U-24). It uses `escrow-core` without changing it: any change to the core changes the wasm of `contracts/escrow-v1` even if that crate does not use it (measured), so budgets, disputes and tickets live in their own crate |
| `contracts/test-ft` | near-contract-standards fungible token, only for the sandbox tests |
| `contracts/registry-v1` | the agent registry (identity + reputation tied to Arras payments); `src/unit_tests/`, `tests/workspaces.rs` (WR-*) |
| `contracts/test-escrow` | a mock escrow whose `get_payment` returns raw JSON, only for the registry's sandbox tests |
| `contracts/erc8004-common`, `erc8004-identity`, `erc8004-reputation`, `erc8004-arras-verifier`, `erc8004-validation` | the ERC-8004 port (see "ERC-8004 on NEAR"); unit tests in each crate's `src/unit_tests/` (in `erc8004-common`, inside each module), the sandbox cycle across all of them in `contracts/erc8004-reputation/tests/workspaces.rs` (WE-*) |
| `contracts/test-nft-receiver` | a mock NFT receiver, only for the Identity Registry's sandbox tests |
| `docs/vectores.json` | golden vectors V-1..V-3 and F-1..F-7; section `v2`: v2 `msg`s, VB-1, tickets T-1/T-2 |
| `scripts/testnet.sh` | the testnet sequence of `contracts/escrow-v1` |
| `scripts/testnet-v2.sh` | the testnet walkthrough of `contracts/escrow-v2` (budgets, disputes, tickets) |
| `scripts/mainnet.sh`, `scripts/test_mainnet.sh` | the mainnet deployment (dry run by default; the same script rehearsed on testnet) and its offline tests |
| `scripts/mainnet-v2.sh` | the mainnet deployment of `v2.arras-escrow.near` and its smoke test (dry run by default; rehearsed on testnet; run on mainnet on 2026-11-08) |
| `scripts/arras-comun.sh` | what both deployment scripts share (networks, flags, caps, transactions, RPC reads, the pinned wasm, keys through a pipe), written once |
| `scripts/test_cadena.py` | the fake chain of the offline tests of `scripts/mainnet-v2.sh` |
| `docs/MAINNET.md` | the mainnet runbook: pinned wasm, accounts and signers, the NEAR it needs (measured), the order with the facilitator, the testnet rehearsal, and the mainnet run of 2026-11-03; section 8, the runbook of `v2.arras-escrow.near`, its testnet rehearsals and its mainnet run of 2026-11-08 |
| `docs/EVIDENCE.md` | the mainnet transactions behind each claim (deployments, Execution Market's test payments and golden flows, the smoke test of `v2.arras-escrow.near`, the KarmaKadabra swarm's agent-to-agent tasks), with explorer links |
| `docs/plans/` | plans written before the code (the ERC-8004 port: `erc8004-near-port.md`); working notes in Spanish |
| `docs/handoffs/` | handoffs from one development session to the next (decisions, what was run, what is left); working notes in Spanish |
| `HACKATHON.md` | the start SHA and the window of Q-001, what existed before 2026-11-01, what was built from then on, and what was deployed when |

Pinned: near-sdk and near-contract-standards `=5.29.1`, near-workspaces `=0.23.0`, cargo-near 0.22.0, near-cli-rs
0.30.1. `Cargo.lock` is committed (the reproducible build requires it).
The workspace skeleton comes from `cargo near new` (cargo-near 0.22.0): `rust-toolchain.toml`, the `cfg(near)` lint
block of `Cargo.toml` and the `[package.metadata.near.reproducible_build]` block of six contract crates.
Built with AI coding assistants, as [Q-005](https://nearhackathon.com/HACKATHON.md) allows.

## License

MIT. See [LICENSE](LICENSE).
